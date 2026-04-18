use super::commands::{close_pending_draw_cmd, DrawCmd};

#[test]
fn close_pending_appends_unclaimed_instances() {
    let mut cmds = vec![DrawCmd {
        first_instance: 0,
        instance_count: 3,
        textured: false,
        texture_idx: 0,
    }];
    close_pending_draw_cmd(&mut cmds, 5, true, 2);

    assert_eq!(cmds.len(), 2);
    assert_eq!(
        cmds[1],
        DrawCmd {
            first_instance: 3,
            instance_count: 2,
            textured: true,
            texture_idx: 2,
        }
    );
}

#[test]
fn close_pending_does_not_duplicate_completed_work() {
    let mut cmds = vec![DrawCmd {
        first_instance: 0,
        instance_count: 4,
        textured: false,
        texture_idx: 0,
    }];
    close_pending_draw_cmd(&mut cmds, 4, true, 1);

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].instance_count, 4);
}
