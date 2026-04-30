use crate::asset::{AssetServer, Handle, TextureAsset, TextureColorSpace};
use crate::gpu::GpuContext;
use crate::render::expert::{Texture, TextureCreateDesc};
use crate::video::playback::rgba_len;
use crate::video::VideoError;

/// Stable texture handle for streamed video frames.
///
/// Decoders can write each decoded RGBA frame into this buffer. Render-facing
/// code keeps using the same [`TextureAsset`] handle, so playback does not
/// allocate one persistent asset per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoFrameBuffer {
    handle: Handle<TextureAsset>,
    width: u32,
    height: u32,
    color_space: TextureColorSpace,
}

impl VideoFrameBuffer {
    pub fn new(
        assets: &AssetServer,
        width: u32,
        height: u32,
        color_space: TextureColorSpace,
    ) -> Result<Self, VideoError> {
        let pixels = vec![0; rgba_len(width, height)?];
        let handle = assets.insert_runtime(TextureAsset::new(width, height, color_space, pixels));
        Ok(Self {
            handle,
            width,
            height,
            color_space,
        })
    }

    #[must_use]
    pub fn handle(self) -> Handle<TextureAsset> {
        self.handle
    }

    #[must_use]
    pub fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(self) -> u32 {
        self.height
    }

    pub fn write_rgba8(
        &self,
        assets: &AssetServer,
        pixels: impl Into<Vec<u8>>,
    ) -> Result<(), VideoError> {
        let pixels = pixels.into();
        let expected = rgba_len(self.width, self.height)?;
        if pixels.len() != expected {
            return Err(VideoError::InvalidFrameDataLength {
                expected,
                actual: pixels.len(),
            });
        }
        assets.replace_runtime(
            self.handle,
            TextureAsset::new(self.width, self.height, self.color_space, pixels),
        )?;
        Ok(())
    }
}

/// GPU-resident streamed video frame target.
///
/// Unlike [`VideoFrameBuffer`], this never routes per-frame updates through the
/// asset system. It owns one stable `wgpu` texture and updates it in-place with
/// `queue.write_texture`, which is the baseline path for real video playback.
#[derive(Clone)]
pub struct GpuVideoFrameBuffer {
    texture: Texture,
    width: u32,
    height: u32,
}

impl GpuVideoFrameBuffer {
    pub fn new(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Self, VideoError> {
        let _ = rgba_len(width, height)?;
        let texture = Texture::create(
            gpu,
            TextureCreateDesc::new_2d(width, height, format)
                .usage(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
                .label("video_stream_frame"),
        );
        Ok(Self {
            texture,
            width,
            height,
        })
    }

    #[must_use]
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn write_rgba8(&self, gpu: &GpuContext, pixels: &[u8]) -> Result<(), VideoError> {
        self.texture.write_rgba8(gpu, pixels)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, TextureColorSpace};

    use super::*;

    #[test]
    fn frame_buffer_reuses_handle_when_pixels_change() {
        let assets = AssetServer::with_empty_manifest(AssetConfig::default());
        let buffer = VideoFrameBuffer::new(&assets, 2, 1, TextureColorSpace::Srgb).unwrap();
        let handle = buffer.handle();

        buffer
            .write_rgba8(&assets, vec![255, 0, 0, 255, 0, 0, 255, 255])
            .unwrap();

        let texture = assets.get(&handle).unwrap();
        assert_eq!(handle, buffer.handle());
        assert_eq!(texture.pixels(), &[255, 0, 0, 255, 0, 0, 255, 255]);
    }
}
