use sky_engine::ecs::World;

use crate::app_bridge::AppRequests;

pub struct ScreenshotProbe {
    frame_count: u32,
    path: Option<String>,
    frame: u32,
    taken: bool,
    exit_after: bool,
}

impl ScreenshotProbe {
    pub fn from_env() -> Self {
        Self {
            frame_count: 0,
            path: std::env::var("SKY_BUILDER_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_BUILDER_SCREENSHOT_FRAME").unwrap_or(16),
            taken: false,
            exit_after: env_flag("SKY_BUILDER_EXIT_AFTER_SCREENSHOT"),
        }
    }

    pub fn update(&mut self, requests: &mut AppRequests) {
        self.frame_count = self.frame_count.wrapping_add(1);
        if requests.skip_render {
            return;
        }
        if !self.taken && self.frame_count >= self.frame {
            if let Some(path) = self.path.as_ref() {
                requests.screenshot = Some(path.clone());
                self.taken = true;
                if self.exit_after {
                    requests.exit = true;
                }
            }
        }
    }
}

pub fn update_screenshot_probe(world: &mut World) {
    let Some(mut probe) = world.remove_resource::<ScreenshotProbe>() else {
        return;
    };
    let Some(requests) = world.get_resource_mut::<AppRequests>() else {
        world.insert_resource(probe);
        return;
    };
    probe.update(requests);
    world.insert_resource(probe);
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse().ok()
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}
