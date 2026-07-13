use sky_engine::ui::serein::State;

use crate::model::GameSession;
use crate::save;

pub fn save_current_state(state: &State<GameSession>) {
    state.update(|session| match save::save(session) {
        Ok(path) => {
            session.notice = Some(format!("已保存到 {}", path.display()));
        }
        Err(error) => {
            session.notice = Some(format!("保存失败：{error}"));
        }
    });
}

pub fn load_into_state(state: &State<GameSession>) {
    state.update(|session| match save::load() {
        Ok(mut loaded) => {
            loaded.notice = Some("存档已读取。".to_string());
            *session = loaded;
        }
        Err(error) => {
            session.notice = Some(format!("读取失败：{error}"));
        }
    });
}
