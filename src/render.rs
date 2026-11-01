//! Affichage wgpu : image calée à gauche, loupe 100 % sous le pointeur,
//! panneau d'informations à droite. Chaque élément est un quad texturé.

use std::sync::Arc;

use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use crate::loader::Decoded;

const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.02,
    a: 1.0,
};
/// Côté de la loupe, en pixels écran (= pixels de l'image, à 100 %).
/// Épaisseur du cadre de la vignette active.
const FRAME_WIDTH: f32 = 4.0;
const LOUPE_SIZE: f32 = 600.0;
const LOUPE_BORDER: f32 = 2.0;
/// Le panneau occupe la place laissée par une image 4:3 en hauteur pleine.
const PANEL_ASPECT: f32 = 4.0 / 3.0;
const PANEL_MIN_WIDTH: f32 = 280.0;
/// Marge à gauche de l'image.
const IMAGE_MARGIN: f32 = 24.0;
const PARAMS_SIZE: u64 = 48;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, (px, py): (f32, f32)) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Un élément dessiné : sa texture (via le bind group) et ses paramètres.
struct Quad {
    params: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct Shown {
    image: Quad,
    loupe: Quad,
    /// Dimensions pleine résolution, après orientation.
    display_w: f32,
    display_h: f32,
    orientation: u32,
    /// Texture pleine résolution : la loupe est possible.
    full: bool,
}

struct PanelTex {
    quad: Quad,
    width: f32,
    height: f32,
}

pub struct Renderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    border: Quad,
    /// Bordure de la loupe figée : bleue, comme la photo active.
    border_pinned: Quad,
    /// Repère de position dans la rafale courante (rectangle blanc).
    marker: Quad,
    marker_rect: Option<Rect>,
    /// Cadre de la vignette active (vue à plat) : un quad par côté.
    frame: [Quad; 4],
    frame_rect: Option<Rect>,
    /// Ascenseur de la liste : piste et curseur.
    track: Quad,
    thumb: Quad,
    scrollbar: Option<(Rect, Rect)>,
    shown: Option<Shown>,
    /// Informations sur l'image courante, en haut à droite.
    panel: Option<PanelTex>,
    /// Liste des rafales, sous les informations.
    list: Option<PanelTex>,
}

impl Renderer {
    pub fn new(window: Arc<Window>, display: OwnedDisplayHandle) -> Self {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display)));
        let surface = instance
            .create_surface(window.clone())
            .expect("création de la surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("aucun GPU compatible");
        log::info!("GPU : {:?}", adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("création du device");

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 1,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Pour le texte du panneau ; les photos sont opaques.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest = device.create_sampler(&Default::default());
        let border = upload(&device, &queue, &[190, 190, 190, 255], 1, 1);
        let border = make_quad(&device, &layout, &nearest, &border);
        let border_pinned = upload(&device, &queue, &[60, 140, 255, 255], 1, 1);
        let border_pinned = make_quad(&device, &layout, &nearest, &border_pinned);
        // Photo active : bleu, en vue rafales (repère) comme à plat (cadre).
        let blue = upload(&device, &queue, &[60, 140, 255, 255], 1, 1);
        let marker = make_quad(&device, &layout, &nearest, &blue);
        let frame = std::array::from_fn(|_| make_quad(&device, &layout, &nearest, &blue));
        let track = upload(&device, &queue, &[40, 40, 40, 255], 1, 1);
        let track = make_quad(&device, &layout, &nearest, &track);
        let thumb = upload(&device, &queue, &[120, 120, 120, 255], 1, 1);
        let thumb = make_quad(&device, &layout, &nearest, &thumb);

        Renderer {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            layout,
            linear,
            nearest,
            border,
            border_pinned,
            marker,
            marker_rect: None,
            frame,
            frame_rect: None,
            track,
            thumb,
            scrollbar: None,
            shown: None,
            panel: None,
            list: None,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn window_size(&self) -> (f32, f32) {
        (self.config.width as f32, self.config.height as f32)
    }

    fn upload(&self, pixels: &[u8], width: u32, height: u32) -> wgpu::TextureView {
        upload(&self.device, &self.queue, pixels, width, height)
    }

    fn quad(&self, view: &wgpu::TextureView, nearest: bool) -> Quad {
        let sampler = if nearest { &self.nearest } else { &self.linear };
        make_quad(&self.device, &self.layout, sampler, view)
    }

    /// Place un quad : `dst` en pixels écran, `src` en [0, 1] dans l'image orientée.
    fn place(&self, quad: &Quad, dst: Rect, src: [f32; 4], orientation: u32) {
        let (ww, wh) = self.window_size();
        let ndc = [
            dst.x / ww * 2.0 - 1.0,
            1.0 - dst.y / wh * 2.0,
            (dst.x + dst.w) / ww * 2.0 - 1.0,
            1.0 - (dst.y + dst.h) / wh * 2.0,
        ];
        let mut params = [0u32; 12];
        for (i, v) in ndc.iter().chain(&src).enumerate() {
            params[i] = v.to_bits();
        }
        params[8] = orientation;
        self.queue
            .write_buffer(&quad.params, 0, bytemuck::cast_slice(&params));
    }

    /// Envoie l'image au GPU ; elle sera affichée au prochain `render`.
    pub fn set_image(&mut self, img: &Decoded, full: bool) {
        let view = self.upload(&img.pixels, img.width, img.height);
        let (w, h) = (img.full_width as f32, img.full_height as f32);
        let (display_w, display_h) = if img.orientation >= 5 { (h, w) } else { (w, h) };
        self.shown = Some(Shown {
            image: self.quad(&view, false),
            loupe: self.quad(&view, true),
            display_w,
            display_h,
            orientation: img.orientation as u32,
            full,
        });
    }

    /// Image ajustée à la zone libre à gauche du panneau (sans dépasser 100 %),
    /// centrée dans cette zone : un 4:3 paysage la remplit, un portrait est au milieu.
    pub fn image_rect(&self) -> Option<Rect> {
        let s = self.shown.as_ref()?;
        let (ww, wh) = self.window_size();
        let area = (ww - IMAGE_MARGIN - self.panel_width() as f32).max(1.0);
        let fit = (area / s.display_w).min(wh / s.display_h).min(1.0);
        let (w, h) = (s.display_w * fit, s.display_h * fit);
        Some(Rect {
            x: (IMAGE_MARGIN + (area - w) / 2.0).round(),
            y: ((wh - h) / 2.0).round(),
            w,
            h,
        })
    }

    /// Point de l'écran en proportions de la photo affichée (0..1), s'il est sur la photo.
    pub fn image_point(&self, screen: (f32, f32)) -> Option<(f32, f32)> {
        let r = self.image_rect()?;
        r.contains(screen).then(|| ((screen.0 - r.x) / r.w, (screen.1 - r.y) / r.h))
    }

    /// Inverse de `image_point`.
    pub fn screen_point(&self, (u, v): (f32, f32)) -> Option<(f32, f32)> {
        let r = self.image_rect()?;
        Some((r.x + u * r.w, r.y + v * r.h))
    }

    /// Largeur du panneau d'informations, à droite de la fenêtre.
    pub fn panel_width(&self) -> u32 {
        let (ww, wh) = self.window_size();
        (ww - IMAGE_MARGIN - wh * PANEL_ASPECT)
            .max(PANEL_MIN_WIDTH)
            .min(ww) as u32
    }

    /// Coin haut-gauche de la liste des rafales, en pixels écran.
    pub fn list_origin(&self) -> (f32, f32) {
        let (ww, _) = self.window_size();
        let top = self.panel.as_ref().map_or(0.0, |p| p.height);
        (ww - self.panel_width() as f32, top)
    }

    /// Repère de position, en pixels écran ; renvoie `true` s'il a bougé.
    pub fn set_marker(&mut self, rect: Option<Rect>) -> bool {
        let changed = rect != self.marker_rect;
        self.marker_rect = rect;
        changed
    }

    /// Cadre de la vignette active, autour de `rect` (pixels écran) ; renvoie `true` s'il a bougé.
    pub fn set_frame(&mut self, rect: Option<Rect>) -> bool {
        let changed = rect != self.frame_rect;
        self.frame_rect = rect;
        changed
    }

    /// Ascenseur (piste, curseur), en pixels écran ; renvoie `true` s'il a changé.
    pub fn set_scrollbar(&mut self, bar: Option<(Rect, Rect)>) -> bool {
        let changed = bar != self.scrollbar;
        self.scrollbar = bar;
        changed
    }

    pub fn set_list(&mut self, pixels: &[u8], width: u32, height: u32) {
        let view = self.upload(pixels, width, height);
        self.list = Some(PanelTex {
            quad: self.quad(&view, true),
            width: width as f32,
            height: height as f32,
        });
    }

    pub fn set_panel(&mut self, pixels: &[u8], width: u32, height: u32) {
        let view = self.upload(pixels, width, height);
        self.panel = Some(PanelTex {
            quad: self.quad(&view, true),
            width: width as f32,
            height: height as f32,
        });
    }

    /// Loupe centrée sur `cursor` : rectangle écran et rectangle source, alignés au pixel.
    fn loupe_rects(&self, cursor: (f32, f32)) -> Option<(Rect, [f32; 4])> {
        let s = self.shown.as_ref().filter(|s| s.full)?;
        let r = self.image_rect()?;
        if !r.contains(cursor) {
            return None;
        }
        let (ww, wh) = self.window_size();
        let size = LOUPE_SIZE.min(ww).min(wh);
        let (sw, sh) = (size.min(s.display_w), size.min(s.display_h));
        // Point de l'image sous le pointeur, en pixels pleine résolution.
        let ix = (cursor.0 - r.x) / r.w * s.display_w;
        let iy = (cursor.1 - r.y) / r.h * s.display_h;
        let sx = (ix - sw / 2.0).round().clamp(0.0, s.display_w - sw);
        let sy = (iy - sh / 2.0).round().clamp(0.0, s.display_h - sh);
        let dst = Rect {
            x: (cursor.0 - sw / 2.0).round().clamp(0.0, ww - sw),
            y: (cursor.1 - sh / 2.0).round().clamp(0.0, wh - sh),
            w: sw,
            h: sh,
        };
        let src = [
            sx / s.display_w,
            sy / s.display_h,
            (sx + sw) / s.display_w,
            (sy + sh) / s.display_h,
        ];
        Some((dst, src))
    }

    /// Dessine la vue ; renvoie `true` si la loupe suit le pointeur (qu'il faut
    /// alors masquer). Une loupe figée (`pinned`) a une bordure bleue.
    pub fn render(&mut self, loupe_at: Option<(f32, f32)>, pinned: bool) -> bool {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                // On dessine quand même ; la reconfiguration suit au prochain resize.
                t
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return false;
            }
            other => {
                log::warn!("surface indisponible : {other:?}");
                return false;
            }
        };
        let view = frame.texture.create_view(&Default::default());

        // Ordre de dessin : image, panneau, puis loupe par-dessus (elle peut déborder).
        let full_src = [0.0, 0.0, 1.0, 1.0];
        let mut draws: Vec<&Quad> = Vec::with_capacity(4);
        if let (Some(s), Some(r)) = (&self.shown, self.image_rect()) {
            self.place(&s.image, r, full_src, s.orientation);
            draws.push(&s.image);
        }
        if let Some(p) = &self.panel {
            let (ww, _) = self.window_size();
            let dst = Rect {
                x: ww - p.width,
                y: 0.0,
                w: p.width,
                h: p.height,
            };
            self.place(&p.quad, dst, full_src, 1);
            draws.push(&p.quad);
        }
        if let Some(l) = &self.list {
            let (x, y) = self.list_origin();
            let dst = Rect {
                x,
                y,
                w: l.width,
                h: l.height,
            };
            self.place(&l.quad, dst, full_src, 1);
            draws.push(&l.quad);
        }
        if let Some(r) = self.marker_rect {
            self.place(&self.marker, r, full_src, 1);
            draws.push(&self.marker);
        }
        if let Some(r) = self.frame_rect {
            let t = FRAME_WIDTH;
            let sides = [
                Rect { x: r.x - t, y: r.y - t, w: r.w + 2.0 * t, h: t },
                Rect { x: r.x - t, y: r.y + r.h, w: r.w + 2.0 * t, h: t },
                Rect { x: r.x - t, y: r.y, w: t, h: r.h },
                Rect { x: r.x + r.w, y: r.y, w: t, h: r.h },
            ];
            for (quad, side) in self.frame.iter().zip(sides) {
                self.place(quad, side, full_src, 1);
                draws.push(quad);
            }
        }
        if let Some((track, thumb)) = self.scrollbar {
            self.place(&self.track, track, full_src, 1);
            self.place(&self.thumb, thumb, full_src, 1);
            draws.push(&self.track);
            draws.push(&self.thumb);
        }
        let mut loupe = false;
        if let Some(s) = &self.shown {
            if let Some((dst, src)) = loupe_at.and_then(|c| self.loupe_rects(c)) {
                let b = LOUPE_BORDER;
                let frame = Rect {
                    x: dst.x - b,
                    y: dst.y - b,
                    w: dst.w + 2.0 * b,
                    h: dst.h + 2.0 * b,
                };
                let border = if pinned { &self.border_pinned } else { &self.border };
                self.place(border, frame, full_src, 1);
                self.place(&s.loupe, dst, src, s.orientation);
                draws.push(border);
                draws.push(&s.loupe);
                loupe = !pinned;
            }
        }

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for quad in draws {
                pass.set_bind_group(0, &quad.bind_group, &[]);
                pass.draw(0..4, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame);
        loupe
    }
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );
    texture.create_view(&Default::default())
}

fn make_quad(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> Quad {
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: PARAMS_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
        ],
    });
    Quad { params, bind_group }
}
