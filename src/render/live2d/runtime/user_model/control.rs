use super::*;

impl Live2DUserModel {
    pub fn set_expression(&mut self, name: &str) -> bool {
        self.expression_player
            .as_mut()
            .is_some_and(|player| player.set_expression(name))
    }

    pub fn set_random_expression(&mut self) -> bool {
        self.expression_player
            .as_mut()
            .is_some_and(Live2DExpressionPlayer::set_random_expression)
    }

    pub fn set_motion(&mut self, group_name: &str, index: usize) -> bool {
        self.motion_player
            .as_mut()
            .is_some_and(|player| player.set_motion(group_name, index))
    }

    pub fn set_motion_by_index(&mut self, index: usize) -> bool {
        self.motion_player
            .as_mut()
            .is_some_and(|player| player.set_motion_by_index(index))
    }

    pub fn start_motion_priority(
        &mut self,
        group_name: &str,
        index: usize,
        priority: MotionPriority,
    ) -> bool {
        self.motion_player
            .as_mut()
            .and_then(|player| player.start_motion_priority(group_name, index, priority))
            .is_some()
    }

    pub fn start_random_motion(&mut self, group_name: &str, priority: MotionPriority) -> bool {
        self.motion_player
            .as_mut()
            .and_then(|player| player.start_random_motion_handle(group_name, priority))
            .is_some()
    }

    pub fn start_motion_priority_handle(
        &mut self,
        group_name: &str,
        index: usize,
        priority: MotionPriority,
    ) -> Option<MotionHandle> {
        self.motion_player
            .as_mut()
            .and_then(|player| player.start_motion_priority(group_name, index, priority))
    }

    pub fn start_random_motion_handle(
        &mut self,
        group_name: &str,
        priority: MotionPriority,
    ) -> Option<MotionHandle> {
        self.motion_player
            .as_mut()
            .and_then(|player| player.start_random_motion_handle(group_name, priority))
    }

    pub fn is_motion_handle_finished(&self, handle: MotionHandle) -> bool {
        self.motion_player
            .as_ref()
            .is_none_or(|player| player.is_finished_handle(handle))
    }

    pub fn take_motion_events(&mut self) -> Vec<MotionFiredEvent> {
        self.motion_player
            .as_mut()
            .map(Live2DMotionPlayer::take_fired_events)
            .unwrap_or_default()
    }

    pub fn take_started_motion_sounds(&mut self) -> Vec<String> {
        self.motion_player
            .as_mut()
            .map(Live2DMotionPlayer::take_started_sounds)
            .unwrap_or_default()
    }

    pub fn take_started_motions(&mut self) -> Vec<MotionStartedEvent> {
        self.motion_player
            .as_mut()
            .map(Live2DMotionPlayer::take_started_motions)
            .unwrap_or_default()
    }

    pub fn take_finished_motions(&mut self) -> Vec<MotionFinishedEvent> {
        self.motion_player
            .as_mut()
            .map(Live2DMotionPlayer::take_finished_motions)
            .unwrap_or_default()
    }

    pub fn set_began_motion_handler<F>(&mut self, handler: F) -> bool
    where
        F: FnMut(&MotionStartedEvent) + 'static,
    {
        self.motion_player.as_mut().is_some_and(|player| {
            player.set_began_motion_handler(handler);
            true
        })
    }

    pub fn clear_began_motion_handler(&mut self) -> bool {
        self.motion_player.as_mut().is_some_and(|player| {
            player.clear_began_motion_handler();
            true
        })
    }

    pub fn set_finished_motion_handler<F>(&mut self, handler: F) -> bool
    where
        F: FnMut(&MotionFinishedEvent) + 'static,
    {
        self.motion_player.as_mut().is_some_and(|player| {
            player.set_finished_motion_handler(handler);
            true
        })
    }

    pub fn clear_finished_motion_handler(&mut self) -> bool {
        self.motion_player.as_mut().is_some_and(|player| {
            player.clear_finished_motion_handler();
            true
        })
    }
}
