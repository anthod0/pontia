use super::*;
use pontia_bot::messages::Todo;

#[tokio::test]
async fn closing_todos_is_atomic_scoped_and_links_the_explanation() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let mut chats = Vec::new();
    for name in ["A", "B"] {
        let bot = service
            .records()
            .create(bot_configuration(name, MODEL))
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
    let answer = service.send(a, "Yes").await.unwrap();
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
        service.open_todos(a).await.unwrap(),
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
    assert!(service.open_todos(a).await.unwrap().is_empty());
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
    assert_eq!(service.open_todos(&chats[1].0).await.unwrap(), [other]);
}
