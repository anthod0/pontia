use pontia_bot::{
    identity::{BotAccess, BotIdentity, BotRole, IdentityError},
    user::*,
};
use std::{future::Future, pin::Pin, sync::Arc};

struct Sessions;
impl BotIdentity for Sessions {
    fn resolve<'a>(
        &'a self,
        session: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<BotAccess, IdentityError>> + Send + 'a>> {
        Box::pin(async move {
            let role = match session {
                "bot-a" | "bot-b" => BotRole::Bot,
                "helper" => BotRole::Helper,
                "retrieval" => BotRole::Retrieval,
                "dreaming" => BotRole::Dreaming,
                _ => return Err(IdentityError),
            };
            Ok(BotAccess {
                role,
                workspace_id: None,
            })
        })
    }
}

struct Fixture {
    service: UserService,
    commands: UserCommands,
    root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let service = UserService::new(root.path());
        let commands = UserCommands::new(service.clone(), Arc::new(Sessions));
        Self {
            service,
            commands,
            root,
        }
    }
    async fn update(&self, session: &str, old: &str, new: &str) -> Result<(), Error> {
        self.commands
            .execute(UserRequest {
                session_id: session.into(),
                input: UpdateUser {
                    old_text: old.into(),
                    new_text: new.into(),
                },
            })
            .await
    }
    fn text(&self) -> String {
        std::fs::read_to_string(self.root.path().join("bot/USER.md")).unwrap()
    }
}

#[tokio::test]
async fn updates_missing_and_empty_user_file_and_preserves_exact_text() {
    let f = Fixture::new();
    assert_eq!(f.service.read_for_handoff().await.unwrap(), "");
    assert!(!f.root.path().join("bot").exists());
    f.update("bot-a", "", "名字：小林\n语言：中文\n")
        .await
        .unwrap();
    f.update("bot-a", "小林", "林").await.unwrap();
    f.update("bot-a", "", "  简洁汇报。\n").await.unwrap();
    f.update("bot-a", "语言：中文\n", "").await.unwrap();
    assert_eq!(f.text(), "名字：林\n  简洁汇报。\n");
    f.update("bot-a", &f.text(), "").await.unwrap();
    assert_eq!(f.service.read_for_handoff().await.unwrap(), "");
    f.update("bot-b", "", "新的认识").await.unwrap();
    assert_eq!(f.text(), "新的认识");
}

#[tokio::test]
async fn rejects_missing_ambiguous_and_overlapping_matches_without_writing() {
    let f = Fixture::new();
    f.update("bot-a", "", "aaaa 中文 中文").await.unwrap();
    let original = f.text();
    for (old, reason) in [
        ("不存在", Rejection::NotFound),
        ("中文", Rejection::Ambiguous),
        ("aaa", Rejection::Ambiguous),
    ] {
        let error = f.update("bot-a", old, "改写").await.unwrap_err();
        match error {
            Error::Rejected {
                reason: actual,
                current_text,
            } => {
                assert_eq!(actual, reason);
                assert_eq!(current_text, original);
            }
            other => panic!("unexpected {other}"),
        }
        assert_eq!(f.text(), original);
    }
}

#[tokio::test]
async fn counts_unicode_characters_and_keeps_file_intact_after_limit_failure() {
    let f = Fixture::new();
    let limit = "文🙂".repeat(1_000);
    f.update("bot-a", "", &limit).await.unwrap();
    assert_eq!(f.text(), limit);
    for (old, new) in [("", "x".to_string()), (limit.as_str(), "字".repeat(2_001))] {
        match f.update("bot-a", old, &new).await.unwrap_err() {
            Error::Rejected {
                reason,
                current_text,
            } => {
                assert_eq!(reason, Rejection::TooLong);
                assert_eq!(current_text, limit);
            }
            other => panic!("unexpected {other}"),
        }
        assert_eq!(f.text(), limit);
    }
    f.update("bot-a", &limit, "").await.unwrap();
    assert_eq!(f.text(), "");
}

#[tokio::test]
async fn concurrent_bots_edit_current_contents_without_lost_updates() {
    let f = Fixture::new();
    f.update("bot-a", "", "名字：林\n语言：中文").await.unwrap();
    let (first, second) = tokio::join!(
        f.update("bot-a", "林", "陈"),
        f.update("bot-b", "中文", "英文"),
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(f.text(), "名字：陈\n语言：英文");
    assert_eq!(f.service.read_for_handoff().await.unwrap(), f.text());
}

#[tokio::test]
async fn only_bound_bots_may_update_user_and_rejections_do_not_disclose_contents() {
    let f = Fixture::new();
    f.update("bot-a", "", "用户私密信息").await.unwrap();
    for session in ["helper", "retrieval", "dreaming", "unknown"] {
        assert!(matches!(
            f.update(session, "不存在", "bad").await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(f.text(), "用户私密信息");
    }
}

#[tokio::test]
async fn invalid_initial_update_leaves_missing_file_missing() {
    let f = Fixture::new();
    assert!(matches!(f.update("bot-a", "missing", "new").await,
        Err(Error::Rejected { reason: Rejection::NotFound, current_text }) if current_text.is_empty()));
    assert!(matches!(f.update("bot-a", "", &"x".repeat(2_001)).await,
        Err(Error::Rejected { reason: Rejection::TooLong, current_text }) if current_text.is_empty()));
    assert!(!f.root.path().join("bot").exists());
}

#[tokio::test]
async fn read_reports_text_limit_and_file_update_time() {
    let f = Fixture::new();
    let missing = f.service.read().await.unwrap();
    assert_eq!(missing.text, "");
    assert_eq!(missing.character_limit, USER_CHARACTER_LIMIT);
    assert!(missing.updated_at.is_none());
    f.update("bot-a", "", "名字：林").await.unwrap();
    let user = f.service.read().await.unwrap();
    assert_eq!(user.text, "名字：林");
    assert!(user.updated_at.is_some());
}
