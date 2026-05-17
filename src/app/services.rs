//! Optional app-owned service installation and per-frame updates.

use crate::ecs::World;

pub(crate) fn install_asset_server(world: &mut World) {
    if world.contains_resource::<crate::asset::AssetServer>() {
        return;
    }

    let config = crate::asset::AssetConfig::default().with_background_loading(true);
    let asset_server = match crate::asset::AssetServer::new(config.clone()) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("[SkyEngine] Asset server initialization failed: {error}");
            crate::asset::AssetServer::with_empty_manifest(config)
        }
    };
    world.insert_resource(asset_server);
}

pub(crate) fn update_assets(world: &World) {
    if let Some(asset_server) = world.get_resource::<crate::asset::AssetServer>().cloned() {
        if let Err(error) = asset_server.update() {
            eprintln!("[SkyEngine] Asset update failed: {error}");
        }
    }
}

pub(crate) fn install_audio(world: &mut World) {
    #[cfg(feature = "audio")]
    {
        let asset_server = ensure_asset_server(world);

        if !world.contains_resource::<crate::audio::AudioServer>() {
            let audio_server =
                crate::audio::AudioServer::new(crate::audio::AudioConfig::default(), asset_server);
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

    #[cfg(not(feature = "audio"))]
    {
        let _ = world;
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
    }

    #[cfg(not(feature = "audio"))]
    {
        let _ = world;
    }
}

pub(crate) fn install_video(world: &mut World) {
    #[cfg(feature = "video")]
    {
        let asset_server = ensure_asset_server(world);

        if !world.contains_resource::<crate::video::VideoServer>() {
            let video_server = crate::video::VideoServer::new(asset_server);
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

    #[cfg(not(feature = "video"))]
    {
        let _ = world;
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
    }

    #[cfg(not(feature = "video"))]
    {
        let _ = (world, frame_delta);
    }
}

#[cfg(any(feature = "audio", feature = "video"))]
fn ensure_asset_server(world: &mut World) -> crate::asset::AssetServer {
    if let Some(server) = world.get_resource::<crate::asset::AssetServer>().cloned() {
        return server;
    }

    let config = crate::asset::AssetConfig::default().with_background_loading(true);
    let server = match crate::asset::AssetServer::new(config.clone()) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("[SkyEngine] Asset server initialization failed: {error}");
            crate::asset::AssetServer::with_empty_manifest(config)
        }
    };
    world.insert_resource(server.clone());
    server
}
