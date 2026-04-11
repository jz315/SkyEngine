#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DrawCmd {
    pub(super) first_instance: u32,
    pub(super) instance_count: u32,
    pub(super) textured: bool,
    pub(super) texture_idx: usize,
}

pub(super) fn close_pending_draw_cmd(
    draw_cmds: &mut Vec<DrawCmd>,
    total_instances: u32,
    textured: bool,
    texture_idx: usize,
) {
    let already_claimed: u32 = draw_cmds.iter().map(|c| c.instance_count).sum();
    let pending = total_instances.saturating_sub(already_claimed);
    if pending > 0 {
        draw_cmds.push(DrawCmd {
            first_instance: already_claimed,
            instance_count: pending,
            textured,
            texture_idx,
        });
    }
}
