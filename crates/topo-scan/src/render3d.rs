//! 3D images for agents: the scan's points and the model's edges, drawn
//! offscreen (no window) by Pangolino's renderer and read back as a PNG.
//! Only with the `pango` feature; without it agents have sections
//! ([`crate::render`]). Nobody's own view is involved.

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use pango_render::renderables::{BufferPolicy, LineList, PointCloud, PointVertex};
use pango_render::{ClearColor, GpuContext, GpuSurface, Group, OffscreenSurface, OrthographicProjection, Projection, RenderGraph, Renderer, SceneView, Transform};

use crate::V3;

/// Where the picture is taken from (scan frame, z up).
pub struct Shot {
    pub eye: V3,
    pub target: V3,
    /// Orthographic with this half-height (m), else perspective (45°).
    pub ortho_half: Option<f32>,
    /// Up in the picture (default: z, or north when looking straight down).
    pub up: Option<V3>,
    pub width: u32,
    pub height: u32,
}

/// A line segment with its colour and whether it is drawn heavy (a highlight).
pub struct Seg {
    pub a: V3,
    pub b: V3,
    pub rgba: [f32; 4],
    pub heavy: bool,
}

/// Draws points (position, RGB) and segments from `shot`; a PNG.
pub fn render(points: &[(V3, [f32; 3])], segs: &[Seg], shot: &Shot) -> Result<Vec<u8>> {
    let (w, h) = (shot.width.clamp(64, 4096), shot.height.clamp(64, 4096));
    let gpu = pollster::block_on(GpuContext::new()).map_err(|e| anyhow::anyhow!("no GPU to draw with: {e}"))?;
    let offscreen = OffscreenSurface::new(&gpu.device, w, h);
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, offscreen.format(), GpuSurface::depth_format());

    let graph = Arc::new(RenderGraph::new());
    let world = graph.add("world", Group);
    let dist = crate::norm(crate::sub(shot.eye, shot.target)).max(0.1);
    // Looking straight down, "up" is north.
    let fwd = crate::scale(crate::sub(shot.target, shot.eye), 1.0 / dist);
    let up = shot.up.unwrap_or(if fwd[2].abs() > 0.999 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] });
    let mut view = SceneView::new().with_world(world).with_look_at_up(shot.eye, shot.target, up);
    view = match shot.ortho_half {
        Some(half) => view.with_projection(Projection::Orthographic(OrthographicProjection::symmetric(half * w as f32 / h as f32, half, 0.05, dist + 500.0))),
        None => view.with_perspective(std::f32::consts::FRAC_PI_4, (dist / 200.0).max(0.05), dist * 20.0 + 200.0),
    };
    let view = view.build(&graph);

    let cloud = PointCloud::new(points.len().max(1)).with_policy(BufferPolicy::Discard).with_points(points.iter().map(|(p, c)| PointVertex::rgb(*p, c[0], c[1], c[2])));
    graph.add_to("scan", cloud, world, Transform::Identity);
    for (name, heavy, width) in [("model", false, 1.5), ("highlight", true, 4.0)] {
        let mut lines = LineList::new(segs.len().max(1)).with_width(width).with_policy(BufferPolicy::Grow);
        for s in segs.iter().filter(|s| s.heavy == heavy) {
            lines.push(s.a, s.b, s.rgba);
        }
        graph.add_to(name, lines, world, Transform::Identity);
    }

    graph.prepare_with(&gpu.device, &gpu.queue, &mut renderer);
    renderer.render_to_offscreen_target(&gpu, &offscreen, ClearColor::new(0.085, 0.09, 0.105), &graph, &[view.target]);

    // Read the picture back (rows of a copy to a buffer are padded to 256 bytes).
    let bytes_per_row = (w * 4).div_ceil(256) * 256;
    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: (bytes_per_row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: offscreen.color_texture(), mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &staging, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(std::iter::once(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    rx.recv().context("reading the picture back")?.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let data = slice.get_mapped_range();
    let bgra = match offscreen.format() {
        wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm => true,
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba8Unorm => false,
        f => bail!("the offscreen picture is {f:?}, which is not read back here"),
    };
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = (y * bytes_per_row + 4 * x) as usize;
            rgb.extend_from_slice(&if bgra { [data[i + 2], data[i + 1], data[i]] } else { [data[i], data[i + 1], data[i + 2]] });
        }
    }
    let mut png_bytes = Vec::new();
    let mut enc = png::Encoder::new(&mut png_bytes, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgb)?;
    Ok(png_bytes)
}
