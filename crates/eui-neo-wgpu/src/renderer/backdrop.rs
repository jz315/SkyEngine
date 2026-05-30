#![allow(unused_imports)]
use super::buffers::*;
use super::collect::*;
use super::images::*;
use super::primitives::*;
use super::text::*;
use super::*;

pub(super) struct CachedBackdrop {
    pub(super) _texture: wgpu::Texture,
    pub(super) _snapshot: Option<wgpu::Texture>,
    pub(super) bind_group: wgpu::BindGroup,
}

pub(super) fn create_dummy_backdrop(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    sampler: &wgpu::Sampler,
    backdrop_texture_layout: &wgpu::BindGroupLayout,
) -> CachedBackdrop {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_dummy_backdrop_texture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
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
        &[0, 0, 0, 0],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_dummy_backdrop_bg"),
        layout: backdrop_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    CachedBackdrop {
        _texture: texture,
        _snapshot: None,
        bind_group,
    }
}

pub(super) fn capture_backdrop(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    op: PrimitiveOp,
    logical_size: [f32; 2],
) -> Option<CachedBackdrop> {
    let source = ctx.target_texture?;
    let [surface_width, surface_height] = ctx.physical_size;
    let capture_rect = backdrop_capture_rect(op, logical_size, [surface_width, surface_height])?;
    let texture_size = [
        ((capture_rect[2] as f32) * 0.5).ceil().max(1.0) as u32,
        ((capture_rect[3] as f32) * 0.5).ceil().max(1.0) as u32,
    ];

    let snapshot = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_backdrop_snapshot_texture"),
        size: wgpu::Extent3d {
            width: surface_width.max(1),
            height: surface_height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ctx.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    ctx.encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: source.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &snapshot,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: surface_width.max(1),
            height: surface_height.max(1),
            depth_or_array_layers: 1,
        },
    );

    let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_backdrop_texture"),
        size: wgpu::Extent3d {
            width: texture_size[0],
            height: texture_size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ctx.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let snapshot_view = snapshot.create_view(&wgpu::TextureViewDescriptor::default());
    let capture_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_backdrop_capture_bg"),
        layout: &renderer.wgpu_resources.image_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&snapshot_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.sampler_linear),
            },
        ],
    });
    write_backdrop_uniform(
        ctx.queue,
        &renderer.wgpu_resources.screen_buffer,
        logical_size,
        [surface_width, surface_height],
        capture_rect,
        texture_size,
    );
    {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let color_attachment = Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        });
        let color_attachments = [color_attachment];
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("eui_neo_backdrop_capture"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&renderer.wgpu_resources.capture_pipeline);
        pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
        pass.set_bind_group(1, &capture_bind_group, &[]);
        draw_fullscreen_triangle(&mut pass);
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_backdrop_bg"),
        layout: &renderer.wgpu_resources.backdrop_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.sampler_linear),
            },
        ],
    });
    Some(CachedBackdrop {
        _texture: texture,
        _snapshot: Some(snapshot),
        bind_group,
    })
}

pub(super) fn backdrop_capture_rect(
    op: PrimitiveOp,
    logical_size: [f32; 2],
    physical_size: [u32; 2],
) -> Option<[f32; 4]> {
    let scale_x = physical_size[0].max(1) as f32 / logical_size[0].max(1.0);
    let scale_y = physical_size[1].max(1) as f32 / logical_size[1].max(1.0);
    let blur = op.backdrop_blur.max(0.0) * scale_x.max(scale_y).max(1.0);
    let frame = op.backdrop_frame;
    let left = ((frame.x * scale_x) - blur)
        .floor()
        .clamp(0.0, physical_size[0].saturating_sub(1).max(1) as f32);
    let top = ((frame.y * scale_y) - blur)
        .floor()
        .clamp(0.0, physical_size[1].saturating_sub(1).max(1) as f32);
    let right = ((frame.right() * scale_x) + blur)
        .ceil()
        .clamp(left + 1.0, physical_size[0].max(1) as f32);
    let bottom = ((frame.bottom() * scale_y) + blur)
        .ceil()
        .clamp(top + 1.0, physical_size[1].max(1) as f32);
    let width = (right - left).max(1.0);
    let height = (bottom - top).max(1.0);
    (width > 0.0 && height > 0.0).then_some([left, top, width, height])
}

pub(super) fn write_backdrop_uniform(
    queue: &wgpu::Queue,
    screen_buffer: &wgpu::Buffer,
    logical_size: [f32; 2],
    physical_size: [u32; 2],
    capture_rect: [f32; 4],
    texture_size: [u32; 2],
) {
    let screen_uniform = ScreenUniform {
        size: [
            logical_size[0],
            logical_size[1],
            physical_size[0].max(1) as f32,
            physical_size[1].max(1) as f32,
        ],
        backdrop_size: [
            texture_size[0].max(1) as f32,
            texture_size[1].max(1) as f32,
            0.0,
            0.0,
        ],
        backdrop_rect: capture_rect,
    };
    queue.write_buffer(screen_buffer, 0, bytemuck::bytes_of(&screen_uniform));
}
