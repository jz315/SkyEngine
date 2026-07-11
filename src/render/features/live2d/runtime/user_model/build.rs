use super::*;

impl Live2DUserModel {
    pub(crate) fn from_resource(resource: &Live2DModelResource) -> Result<Self, Live2DLoadError> {
        let mut model =
            Live2DModel::from_moc3_bytes(&resource.moc_bytes).map_err(Live2DLoadError::Model)?;
        if let Some(layout) = resource.layout {
            model.apply_layout(&layout);
        }
        model.register_virtual_parameter_ids(&resource.extra_parameter_ids);

        Ok(Self {
            model,
            motion_player: resource
                .motion_player_template
                .as_ref()
                .map(Live2DMotionPlayer::fresh_clone),
            eye_blink: resource.eye_blink_template.clone(),
            expression_player: resource
                .expression_player_template
                .as_ref()
                .map(Live2DExpressionPlayer::fresh_clone),
            look: resource.look_template.clone(),
            breath: resource.breath_template.clone(),
            physics: resource.physics_template.clone(),
            lip_sync: resource.lip_sync_template.clone(),
            pose: resource.pose_template.clone(),
            hit_areas: resource.hit_areas.clone(),
        })
    }
}
