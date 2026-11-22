use super::*;
use pontia_bot::messages::{MessageKind, MessageLinks, MessageService};

#[tokio::test]
async fn link_message_replaces_a_whole_set_of_earlier_messages_in_this_chat() {
    let (_root, pool, _client, _control, service) = fixture().await;
    let (chat, session) = bot(&service).await;
    let (other_chat, _) = bot(&service).await;
    let messages = MessageService::new(pool.clone());
    let mut earlier = Vec::new();
    for _ in 0..6 {
        earlier.push(
            messages
                .append(&chat, MessageKind::BotReply, "Earlier")
                .await
                .unwrap()
                .id,
        );
    }
    let input = messages
        .append(&chat, MessageKind::UserInput, "Continue")
        .await
        .unwrap();
    let later = messages
        .append(&chat, MessageKind::UserInput, "Later")
        .await
        .unwrap();
    let foreign = messages
        .append(&other_chat, MessageKind::UserInput, "Other")
        .await
        .unwrap();
    sqlx::query("UPDATE bot_messages SET created_at = '2026-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE bot_messages SET status = 'failed' WHERE id = ?")
        .bind(&input.id)
        .execute(&pool)
        .await
        .unwrap();
    let links = |source: &str, targets: &[String]| MessageLinks {
        message_id: source.into(),
        related_to: targets.iter().cloned().collect(),
    };
    let mut events = service.subscribe(&chat);
    service
        .link_message(&session, &links(&input.id, &earlier[..5]))
        .await
        .unwrap();
    assert!(matches!(
        events.recv().await.unwrap(),
        ConversationUpdate::Changed
    ));
    assert_eq!(
        service
            .message_context(&chat, &input.id)
            .await
            .unwrap()
            .messages
            .len(),
        6
    );
    service
        .link_message(&session, &links(&input.id, &earlier[5..]))
        .await
        .unwrap();
    for targets in [
        vec![],
        earlier.clone(),
        vec![input.id.clone()],
        vec![later.id],
        vec![earlier[0].clone(), foreign.id.clone()],
        vec!["missing".into()],
    ] {
        assert!(
            service
                .link_message(&session, &links(&input.id, &targets))
                .await
                .is_err()
        );
        let ids: Vec<_> = service
            .message_context(&chat, &input.id)
            .await
            .unwrap()
            .messages
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, [earlier[5].clone(), input.id.clone()]);
    }
    for source in [&foreign.id, &earlier[1], "missing"] {
        assert!(
            service
                .link_message(&session, &links(source, &earlier[..1]))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn explicit_quotes_are_protected_even_without_a_stored_edge() {
    let (_root, pool, _client, _control, service) = fixture().await;
    let (chat, session) = bot(&service).await;
    let messages = MessageService::new(pool.clone());
    let target = messages
        .append(&chat, MessageKind::BotReply, "Original")
        .await
        .unwrap();
    for attributes in [
        format!("message_id=\"{}\" speaker=\"Bot\"", target.id),
        "message_id=\"bmsg_0193a000-0000-7000-8000-000000000002\" speaker=\"Bot\"".into(),
        format!(
            "message_id=\"{}\" speaker=\"Bot\" path=\"/work/file\"",
            target.id
        ),
    ] {
        let content = format!("<user_quote {attributes}>Original</user_quote>\nExplain");
        let input = messages
            .append(&chat, MessageKind::UserInput, &content)
            .await
            .unwrap();
        let links = MessageLinks {
            message_id: input.id,
            related_to: [target.id.clone()].into(),
        };
        let source: String =
            sqlx::query_scalar("SELECT link_source FROM bot_messages WHERE id = ?")
                .bind(&links.message_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(source, "quote");
        assert!(service.link_message(&session, &links).await.is_err());
        // A historical quote without an edge was not classified by the migration.
        sqlx::query("UPDATE bot_messages SET link_source = NULL WHERE id = ?")
            .bind(&links.message_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(service.link_message(&session, &links).await.is_err());
    }
}
