use crate::asset::{Asset, Assets, Handle};
use crate::ecs::World;
use crate::render::SpriteRenderer;

/// One UV frame in a sprite animation clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteAnimationFrame {
    pub uv: [f32; 4],
    pub duration_ms: u32,
}

impl SpriteAnimationFrame {
    #[inline]
    pub const fn new(uv: [f32; 4], duration_ms: u32) -> Self {
        Self { uv, duration_ms }
    }
}

/// Shared immutable UV animation data for sprites.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteAnimationClip {
    pub frames: Vec<SpriteAnimationFrame>,
    pub duration_ms: u32,
}

impl Asset for SpriteAnimationClip {
    const TYPE: &'static str = "sky.sprite_animation_clip";
}

impl SpriteAnimationClip {
    pub fn new(frames: impl Into<Vec<SpriteAnimationFrame>>) -> Self {
        let frames = frames.into();
        let duration_ms = frames
            .iter()
            .map(|frame| frame.duration_ms.max(1))
            .sum::<u32>()
            .max(1);
        Self {
            frames,
            duration_ms,
        }
    }

    pub fn frame_index_at(&self, elapsed_seconds: f32) -> Option<usize> {
        if self.frames.is_empty() {
            return None;
        }
        let phase_ms = looped_phase_ms(elapsed_seconds, self.duration_ms);
        Some(frame_index_for_phase(&self.frames, phase_ms))
    }

    pub fn frame_uv_at(&self, elapsed_seconds: f32) -> Option<[f32; 4]> {
        self.frame_index_at(elapsed_seconds)
            .map(|index| self.frames[index].uv)
    }
}

/// Per-entity playback state for a [`SpriteAnimationClip`].
#[derive(Clone, Debug)]
pub struct SpriteAnimator {
    pub clip: Handle<SpriteAnimationClip>,
    pub elapsed_seconds: f32,
    pub playing: bool,
    pub repeat: bool,
    pub speed: f32,
}

impl SpriteAnimator {
    #[inline]
    pub fn new(clip: Handle<SpriteAnimationClip>) -> Self {
        Self {
            clip,
            elapsed_seconds: 0.0,
            playing: true,
            repeat: true,
            speed: 1.0,
        }
    }

    #[inline]
    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    #[inline]
    pub fn repeat(mut self, repeat: bool) -> Self {
        self.repeat = repeat;
        self
    }

    #[inline]
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    #[inline]
    pub fn reset(mut self) -> Self {
        self.elapsed_seconds = 0.0;
        self
    }
}

/// Advance sprite UV animations and write the active frame into [`SpriteRenderer`].
pub fn animate_sprites(world: &mut World) {
    let Some(asset_server) = world.get_resource::<Assets>().cloned() else {
        return;
    };
    let delta_seconds = world.time.delta.max(0.0);
    let mut query = world.query_mut::<(&mut SpriteRenderer, &mut SpriteAnimator)>();
    query.for_each(|(sprite, animator)| {
        let Some(clip) = asset_server.try_get(&animator.clip) else {
            return;
        };
        if clip.frames.is_empty() {
            return;
        }

        if animator.playing && animator.speed > 0.0 {
            animator.elapsed_seconds += delta_seconds * animator.speed;
        }

        let Some(uv) = animator_frame_uv(animator, &clip) else {
            return;
        };
        sprite.uv = uv;
    });
}

fn animator_frame_uv(
    animator: &mut SpriteAnimator,
    clip: &SpriteAnimationClip,
) -> Option<[f32; 4]> {
    if animator.repeat {
        return clip.frame_uv_at(animator.elapsed_seconds);
    }

    let duration_ms = clip.duration_ms.max(1);
    let elapsed_ms = (animator.elapsed_seconds.max(0.0) * 1000.0) as u32;
    let phase_ms = elapsed_ms.min(duration_ms.saturating_sub(1));
    if elapsed_ms >= duration_ms {
        animator.elapsed_seconds = duration_ms as f32 / 1000.0;
        animator.playing = false;
    }
    let index = frame_index_for_phase(&clip.frames, phase_ms);
    Some(clip.frames[index].uv)
}

fn looped_phase_ms(elapsed_seconds: f32, duration_ms: u32) -> u32 {
    let duration = duration_ms.max(1) as u64;
    let elapsed = (elapsed_seconds.max(0.0) * 1000.0) as u64;
    (elapsed % duration) as u32
}

fn frame_index_for_phase(frames: &[SpriteAnimationFrame], phase_ms: u32) -> usize {
    let mut phase = phase_ms;
    for (index, frame) in frames.iter().enumerate() {
        let duration = frame.duration_ms.max(1);
        if phase < duration {
            return index;
        }
        phase = phase.saturating_sub(duration);
    }
    frames.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, AssetId, Assets, Handle};
    use crate::render::SpriteRenderer;

    use super::*;

    fn clip() -> SpriteAnimationClip {
        SpriteAnimationClip::new([
            SpriteAnimationFrame::new([0.0, 0.0, 0.5, 0.5], 100),
            SpriteAnimationFrame::new([0.5, 0.0, 1.0, 0.5], 100),
            SpriteAnimationFrame::new([0.0, 0.5, 0.5, 1.0], 200),
        ])
    }

    #[test]
    fn sprite_animation_clip_selects_uv_by_duration() {
        let clip = clip();
        assert_eq!(clip.frame_uv_at(0.0), Some([0.0, 0.0, 0.5, 0.5]));
        assert_eq!(clip.frame_uv_at(0.1), Some([0.5, 0.0, 1.0, 0.5]));
        assert_eq!(clip.frame_uv_at(0.2), Some([0.0, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn animate_sprites_switches_uv_and_loops() {
        let asset_server = Assets::with_empty_manifest(AssetConfig::default());
        let handle = asset_server.insert_runtime(clip());
        let mut world = World::new();
        world.insert_resource(asset_server);
        let entity = world.spawn((SpriteRenderer::new(16.0, 16.0), SpriteAnimator::new(handle)));

        world.time.delta = 0.4;
        animate_sprites(&mut world);

        let sprite = world.get::<SpriteRenderer>(entity).unwrap();
        assert_eq!(sprite.uv, [0.0, 0.0, 0.5, 0.5]);
    }

    #[test]
    fn animate_sprites_clamps_non_repeating_animation() {
        let asset_server = Assets::with_empty_manifest(AssetConfig::default());
        let handle = asset_server.insert_runtime(clip());
        let mut world = World::new();
        world.insert_resource(asset_server);
        let entity = world.spawn((
            SpriteRenderer::new(16.0, 16.0),
            SpriteAnimator::new(handle).repeat(false),
        ));

        world.time.delta = 1.0;
        animate_sprites(&mut world);

        let sprite = world.get::<SpriteRenderer>(entity).unwrap();
        let animator = world.get::<SpriteAnimator>(entity).unwrap();
        assert_eq!(sprite.uv, [0.0, 0.5, 0.5, 1.0]);
        assert!(!animator.playing);
    }

    #[test]
    fn animate_sprites_respects_playing_and_speed() {
        let asset_server = Assets::with_empty_manifest(AssetConfig::default());
        let handle = asset_server.insert_runtime(clip());
        let mut world = World::new();
        world.insert_resource(asset_server);
        let paused = world.spawn((
            SpriteRenderer::new(16.0, 16.0),
            SpriteAnimator::new(handle.clone()).playing(false),
        ));
        let stopped = world.spawn((
            SpriteRenderer::new(16.0, 16.0),
            SpriteAnimator::new(handle.clone()).speed(0.0),
        ));
        let fast = world.spawn((
            SpriteRenderer::new(16.0, 16.0),
            SpriteAnimator::new(handle).speed(2.0),
        ));

        world.time.delta = 0.1;
        animate_sprites(&mut world);

        assert_eq!(
            world.get::<SpriteRenderer>(paused).unwrap().uv,
            [0.0, 0.0, 0.5, 0.5]
        );
        assert_eq!(
            world.get::<SpriteRenderer>(stopped).unwrap().uv,
            [0.0, 0.0, 0.5, 0.5]
        );
        assert_eq!(
            world.get::<SpriteRenderer>(fast).unwrap().uv,
            [0.0, 0.5, 0.5, 1.0]
        );
    }

    #[test]
    fn animate_sprites_skips_missing_inputs() {
        let missing = Handle::<SpriteAnimationClip>::new(AssetId::new());
        let mut world = World::new();
        world.insert_resource(Assets::with_empty_manifest(AssetConfig::default()));
        world.spawn((
            SpriteRenderer::new(16.0, 16.0),
            SpriteAnimator::new(missing.clone()),
        ));
        world.time.delta = 0.1;
        animate_sprites(&mut world);

        let mut world = World::new();
        world.spawn((
            SpriteRenderer::new(16.0, 16.0),
            SpriteAnimator::new(missing),
        ));
        animate_sprites(&mut world);
    }
}
