use std::f32::consts::PI;
use std::path::Path;

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::asset::{cook, AssetConfig, AssetServer};
use sky_engine::audio::{
    AudioBusId, AudioConfig as EngineAudioConfig, AudioEmitter2D, AudioListener2D,
    AudioPlaybackSettings, AudioServer, MusicTrack, SoundClip,
};
use sky_engine::ecs::World;
use sky_engine::render::Transform;

struct DemoState {
    elapsed: f32,
    emitter: sky_engine::ecs::EntityId,
}

impl AppState for DemoState {
    fn update(&mut self, ctx: &mut FrameContext) {
        self.elapsed += ctx.dt;
        if let Some(transform) = ctx.world.get_mut::<Transform>(self.emitter) {
            transform.position[0] = self.elapsed.cos() * 4.0;
            transform.position[1] = (self.elapsed * 0.5).sin() * 2.0;
        }
        ctx.set_title(&format!("audio_demo | t={:.2}s", self.elapsed));
        if self.elapsed > 8.0 {
            ctx.request_exit();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let asset_root = temp.path().join("assets");
    std::fs::create_dir_all(&asset_root)?;

    let sfx_path = asset_root.join("sfx_beep.wav");
    let music_path = asset_root.join("music_bgm.wav");
    write_wav(&sfx_path, 0.5, 660.0)?;
    write_wav(&music_path, 2.0, 220.0)?;

    let asset_config = AssetConfig::new(&asset_root, "native");
    let _ = cook::import_path(&asset_root, &sfx_path)?;
    let _ = cook::import_path(&asset_root, &music_path)?;
    let _ = cook::cook_all(&asset_config)?;

    let asset_server = AssetServer::new(asset_config)?;
    let audio_server = AudioServer::new(EngineAudioConfig::default(), asset_server.clone());
    let audio_commands = audio_server.commands();

    let sfx = asset_server.load_by_path::<SoundClip>(&sfx_path)?;
    let music = asset_server.load_by_path::<MusicTrack>(&music_path)?;
    asset_server.update()?;
    let _ = asset_server.get(&sfx)?;
    let _ = asset_server.get(&music)?;

    let _ = audio_server.play_music(
        music,
        AudioPlaybackSettings::default()
            .on_bus(AudioBusId::MUSIC)
            .gain(0.5)
            .looped(true),
    );

    let mut world = World::new();
    world.insert_resource(asset_server);
    world.insert_resource(audio_server.clone());
    world.insert_resource(audio_commands);

    world.spawn((AudioListener2D::default(), Transform::default()));
    let emitter = world.spawn((
        Transform::from_xy(4.0, 0.0),
        AudioEmitter2D::sound(sfx).looped(true),
    ));

    App::new(AppConfig::new("audio_demo", 960, 540), world).run(DemoState {
        elapsed: 0.0,
        emitter,
    });
    Ok(())
}

fn write_wav(path: &Path, seconds: f32, frequency: f32) -> Result<(), Box<dyn std::error::Error>> {
    let sample_rate = 44_100u32;
    let frames = (seconds * sample_rate as f32) as u32;
    let mut data = Vec::with_capacity(frames as usize * 2);

    for frame in 0..frames {
        let t = frame as f32 / sample_rate as f32;
        let sample = (t * frequency * 2.0 * PI).sin() * 0.35;
        let pcm = (sample * i16::MAX as f32) as i16;
        data.extend_from_slice(&pcm.to_le_bytes());
    }

    let byte_rate = sample_rate * 2;
    let block_align = 2u16;
    let data_len = data.len() as u32;
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(path, wav)?;
    Ok(())
}

trait DemoEmitterExt {
    fn looped(self, looped: bool) -> Self;
}

impl DemoEmitterExt for AudioEmitter2D {
    fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }
}
