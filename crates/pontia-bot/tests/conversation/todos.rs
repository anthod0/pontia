use super::*;
use pontia_bot::messages::Todo;

#[tokio::test]
async fn closing_todos_is_atomic_scoped_and_links_the_explanation() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let mut chats = Vec::new();
    for name in ["A", "B"] {
        let bot = service
            .records()
            .create(bot_configuration(name, MODEL), Default::default())
            .await
            .unwrap();
        service.send(&bot.bot_id, "Plan the release").await.unwrap();
        let session = service
            .records()
            .get(&bot.bot_id)
            .await
            .unwrap()
            .current_session_id
            .unwrap();
        chats.push((bot.bot_id, session));
    }
    let reply = |chat: usize, message: &str, todo, targets: &[&str]| ReplyRequest {
        session_id: chats[chat].1.clone(),
        input: Reply {
            message: message.to_owned(),
            todo,
            related_to: vec![],
            resolve_todos: targets.iter().map(|s| (*s).to_owned()).collect(),
        },
    };
    let a = &chats[0].0;
    let first = service
        .send_to_user(reply(0, "Renew?", true, &[]))
        .await
        .unwrap()
        .message;
    let second = service
        .send_to_user(reply(0, "Sign?", true, &[]))
        .await
        .unwrap()
        .message;
    let other = service
        .send_to_user(reply(1, "Approve?", true, &[]))
        .await
        .unwrap()
        .message;
    let answer = service.send(a, "Yes").await.unwrap().message;
    let before = service.chat(a).await.unwrap().messages;
    for invalid in [&other.id, &answer.id, "missing"] {
        assert!(
            service
                .send_to_user(reply(0, "Closing both", true, &[&first.id, invalid]))
                .await
                .is_err()
        );
        assert_eq!(service.chat(a).await.unwrap().messages, before);
    }
    assert_eq!(
        service.todos(a).await.unwrap().open,
        [second.clone(), first.clone()]
    );
    let mut close = reply(
        0,
        "Renewed and signed",
        false,
        &[&first.id, &second.id, &first.id],
    );
    close.input.related_to.push(first.id.clone());
    let sent = service.send_to_user(close).await.unwrap();
    assert_eq!(sent.resolved, [first.id.clone(), second.id.clone()]);
    assert!(sent.already_resolved.is_empty());
    assert!(service.todos(a).await.unwrap().open.is_empty());
    for original in [first, second] {
        let context = service
            .message_context(a, &original.id)
            .await
            .unwrap()
            .messages;
        assert_eq!(context.len(), 2);
        assert!(
            matches!(&context[0].todo, Some(Todo::Done { by, at, message_id: Some(id) }) if by == "bot" && !at.is_empty() && id == &sent.message.id)
        );
        assert_eq!(context[1], sent.message);
        assert!(
            service
                .send_to_user(reply(0, "Already closed", false, &[&original.id]))
                .await
                .is_err()
        );
    }
    assert_eq!(service.todos(&chats[1].0).await.unwrap().open, [other]);
}

#[tokio::test]
async fn user_todo_changes_notify_once_and_preserve_who_closed_it() {
    use pontia_bot::messages::TodoAction::{Mark, Resolve};
    let (_root, pool, _client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Planner", MODEL), Default::default())
        .await
        .unwrap();
    let id = &bot.bot_id;
    let user = service.send(id, "Plan").await.unwrap().message;
    let session = service
        .records()
        .get(id)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let reply = |message, todo, resolve_todos| ReplyRequest {
        session_id: session.clone(),
        input: Reply {
            message,
            todo,
            related_to: vec![],
            resolve_todos,
        },
    };
    let (marked, changed) = service
        .session_todo(&session, &user.id, Mark)
        .await
        .unwrap();
    assert!(changed && matches!(marked.todo, Some(Todo::Open {by,..}) if by == "bot"));
    assert!(
        !service
            .session_todo(&session, &user.id, Mark)
            .await
            .unwrap()
            .1
    );
    let (closed, changed) = service
        .session_todo(&session, &user.id, Resolve)
        .await
        .unwrap();
    assert!(
        changed && matches!(closed.todo, Some(Todo::Done {by, message_id:None,..}) if by == "bot")
    );
    service.change_todo(id, &user.id, Mark).await.unwrap();
    let content = format!("One\n\t{}", "字".repeat(100));
    let todo = service
        .send_to_user(reply(content, true, vec![]))
        .await
        .unwrap()
        .message
        .id;
    assert!(matches!(
        service.change_todo("another-bot", &todo, Resolve).await,
        Err(pontia_core::Error::NotFound(_))
    ));
    let (first, duplicate) = tokio::join!(
        service.change_todo(id, &todo, Resolve),
        service.change_todo(id, &todo, Resolve)
    );
    let first = first.unwrap();
    assert_eq!(duplicate.unwrap(), first);
    assert!(matches!(&first.todo, Some(Todo::Done { by, message_id: None, .. }) if by == "user"));
    assert_eq!(service.todos(id).await.unwrap().recently_resolved, [first]);
    let sent = service
        .send_to_user(reply("Acknowledged".into(), false, vec![todo.clone()]))
        .await
        .unwrap();
    assert_eq!(sent.already_resolved, [todo.as_str()]);
    for _ in 0..2 {
        assert!(
            matches!(service.change_todo(id, &todo, Mark).await.unwrap().todo, Some(Todo::Open {by,..}) if by == "user")
        );
    }
    assert!(
        service
            .todos(id)
            .await
            .unwrap()
            .recently_resolved
            .is_empty()
    );
    service
        .send_to_user(reply("Resolved".into(), false, vec![todo.clone()]))
        .await
        .unwrap();
    assert!(matches!(
        service.change_todo(id, &todo, Mark).await.unwrap().todo,
        Some(Todo::Open {by,..}) if by == "user"
    ));
    let persisted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM bot_notifications WHERE bot_id = ? AND id LIKE 'todo:%'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, 0);
}
