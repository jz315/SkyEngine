//! Optional app-owned service installation and per-frame updates.

use crate::ecs::World;

pub(crate) fn install_assets(world: &mut World, config: crate::asset::AssetConfig) {
    if world.contains_resource::<crate::asset::Assets>() {
        return;
    }

    let assets = match crate::asset::Assets::new(config.clone()) {
        Ok(assets) => assets,
        Err(error) => {
            eprintln!("[SkyEngine] Assets initialization failed: {error}");
            crate::asset::Assets::with_empty_manifest(config)
        }
    };
    world.insert_resource(assets);
}

pub(crate) fn update_assets(world: &mut World) {
    if let Some(assets) = world.get_resource::<crate::asset::Assets>().cloned() {
        if let Err(error) = assets.update() {
            log::error!(target: "sky_engine::asset", "asset update failed: {error}");
        }
        crate::app::asset_diagnostics::publish_asset_diagnostics(world, &assets);
    }
}

#[cfg(feature = "audio")]
pub(crate) fn install_audio(world: &mut World, config: crate::audio::AudioConfig) {
    let assets = ensure_assets(world);
    crate::audio::register_audio_asset_factories(&assets);

    if !world.contains_resource::<crate::audio::AudioServer>() {
        let audio_server = crate::audio::AudioServer::new(config, assets);
        let audio_commands = audio_server.commands();
        world.insert_resource(audio_server);
        if !world.contains_resource::<crate::audio::AudioCommands>() {
            world.insert_resource(audio_commands);
        }
    } else if !world.contains_resource::<crate::audio::AudioCommands>() {
        if let Some(audio_server) = world.get_resource::<crate::audio::AudioServer>().cloned() {
            world.insert_resource(audio_server.commands());
        }
    }
}

pub(crate) fn update_audio_after_frame(world: &mut World) {
    #[cfg(feature = "audio")]
    if let Some(audio_server) = world.get_resource::<crate::audio::AudioServer>().cloned() {
        if let Err(error) = audio_server.apply_commands() {
            eprintln!("[SkyEngine] Audio command application failed: {error}");
        }

        if let Err(error) = audio_server.sync_world(world) {
            eprintln!("[SkyEngine] Audio world sync failed: {error}");
        }
        audio_server.update();
        crate::app::media_diagnostics::publish_audio_diagnostics(world, &audio_server);
    }

    #[cfg(not(feature = "audio"))]
    {
        let _ = world;
    }
}

#[cfg(feature = "video")]
pub(crate) fn install_video(world: &mut World) {
    let assets = ensure_assets(world);
    crate::video::register_video_asset_factories(&assets);

    if !world.contains_resource::<crate::video::VideoServer>() {
        let video_server = crate::video::VideoServer::new(assets);
        let video_commands = video_server.commands();
        world.insert_resource(video_server);
        if !world.contains_resource::<crate::video::VideoCommands>() {
            world.insert_resource(video_commands);
        }
    } else if !world.contains_resource::<crate::video::VideoCommands>() {
        if let Some(video_server) = world.get_resource::<crate::video::VideoServer>().cloned() {
            world.insert_resource(video_server.commands());
        }
    }
}

pub(crate) fn update_video(world: &mut World, frame_delta: f32) {
    #[cfg(feature = "video")]
    if let Some(video_server) = world.get_resource::<crate::video::VideoServer>().cloned() {
        if let Err(error) = video_server.apply_commands() {
            eprintln!("[SkyEngine] Video command application failed: {error}");
        }
        video_server.update(frame_delta);
        if let Err(error) = video_server.sync_world(world) {
            eprintln!("[SkyEngine] Video world sync failed: {error}");
        }
        crate::app::media_diagnostics::publish_video_diagnostics(world, &video_server);
    }

    #[cfg(not(feature = "video"))]
    {
        let _ = (world, frame_delta);
    }
}

#[cfg(any(feature = "audio", feature = "video"))]
fn ensure_assets(world: &mut World) -> crate::asset::Assets {
    if let Some(assets) = world.get_resource::<crate::asset::Assets>().cloned() {
        return assets;
    }

    let config = crate::asset::AssetConfig::default().with_background_loading(true);
    let assets = match crate::asset::Assets::new(config.clone()) {
        Ok(assets) => assets,
        Err(error) => {
            eprintln!("[SkyEngine] Assets initialization failed: {error}");
            crate::asset::Assets::with_empty_manifest(config)
        }
    };
    world.insert_resource(assets.clone());
    assets
}

#[cfg(test)]
mod tests {
    #[cfg(any(feature = "audio", feature = "video"))]
    use crate::diagnostics::Diagnostics;
    #[cfg(any(feature = "audio", feature = "video"))]
    use crate::ecs::World;

    #[cfg(any(feature = "audio", feature = "video"))]
    use super::*;

    #[cfg(feature = "audio")]
    #[test]
    fn update_audio_after_frame_publishes_audio_stats_diagnostics() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let audio = crate::audio::AudioServer::new(
            crate::audio::AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets,
        );
        let mut world = World::new();
        world.insert_resource(audio);

        update_audio_after_frame(&mut world);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        assert!(diagnostics
            .events()
            .iter()
            .any(|event| event.code == "audio.stats"));
    }

    #[cfg(feature = "video")]
    #[test]
    fn update_video_publishes_video_stats_diagnostics() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let texture = assets.insert_runtime(crate::asset::TextureAsset::white_pixel());
        let clip = crate::video::VideoClip::from_textures(1, 1, 10.0, [texture]).unwrap();
        let clip = assets.insert_runtime(clip);
        let video = crate::video::VideoServer::new(assets);
        video
            .play(clip, crate::video::VideoPlaybackSettings::default())
            .unwrap();
        let mut world = World::new();
        world.insert_resource(video);

        update_video(&mut world, 0.016);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        assert!(diagnostics
            .events()
            .iter()
            .any(|event| event.code == "video.stats"));
    }
}
