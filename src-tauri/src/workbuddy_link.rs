use crate::{engine::ApplyResult, model::AppResult};
/// Open the new-task page without claiming that WorkBuddy changed its selected model.
pub fn new_task_url() -> &'static str {
    "workbuddy://home"
}

/// Report the handoff separately from the committed configuration and require manual selection.
pub fn finish_selection(result: &mut ApplyResult, open: impl FnOnce(&str) -> AppResult<()>) {
    let Some(_model_id) = result.workbuddy_model_id.take() else {
        return;
    };
    result.workbuddy_selection = Some(match open(new_task_url()) {
        Ok(()) => "已请求打开 WorkBuddy 新建任务页；请在模型列表中手动选中刚写入的模型。".into(),
        Err(_) => "无法唤起 WorkBuddy；配置已写入，请打开新建任务页手动选择模型。".into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open only the new-task page; no model ID or credential is sent in the URL.
    #[test]
    fn new_task_link_has_no_model_query() {
        let parsed = url::Url::parse(new_task_url()).unwrap();
        assert_eq!(parsed.scheme(), "workbuddy");
        assert_eq!(parsed.host_str(), Some("home"));
        assert!(parsed.query().is_none());
    }

    /// A requested handoff opens home once and its failure never changes commit success.
    #[test]
    fn launch_outcome_is_separate_from_commit() {
        let mut result = ApplyResult {
            backup_id: "backup".into(),
            paths: vec![],
            message: "配置已写入".into(),
            workbuddy_selection: None,
            workbuddy_model_id: Some("model/x".into()),
        };
        let mut calls = 0;
        finish_selection(&mut result, |url| {
            calls += 1;
            assert_eq!(url, "workbuddy://home");
            Err("scheme missing".into())
        });
        assert_eq!(calls, 1);
        assert_eq!(result.message, "配置已写入");
        assert!(result.workbuddy_selection.unwrap().contains("无法唤起"));
        assert!(result.workbuddy_model_id.is_none());
    }

    /// A commit without the WorkBuddy selection intent must not invoke the OS opener.
    #[test]
    fn no_selection_intent_never_opens_link() {
        let mut result = ApplyResult {
            backup_id: "backup".into(),
            paths: vec![],
            message: "配置已写入".into(),
            workbuddy_selection: None,
            workbuddy_model_id: None,
        };
        finish_selection(&mut result, |_| panic!("不应唤起 WorkBuddy"));
        assert!(result.workbuddy_selection.is_none());
    }
}
