use super::*;

const MESSAGE_ID: &str = "bmsg_0193a000-0000-7000-8000-000000000002";

fn quoted(text: &str, excerpt: bool) -> String {
    let flag = if excerpt { " excerpt=\"true\"" } else { "" };
    format!(
        "<user_quote message_id=\"{MESSAGE_ID}\" speaker=\"Bot\"{flag}>{text}</user_quote>\nExplain"
    )
}

fn file_quote(name: &str) -> String {
    format!(
        "<user_quote message_id=\"{MESSAGE_ID}\" speaker=\"Bot\" path=\"/work/{name}\">{name}</user_quote>\nExplain"
    )
}

#[tokio::test]
async fn new_quotes_are_capped_before_storage_and_bot_delivery() {
    let (_root, _pool, client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    let cases = [
        ("a".repeat(2000), "a".repeat(2000), false),
        ("a".repeat(2001), format!("{}…", "a".repeat(2000)), true),
        ("😀".repeat(2000), "😀".repeat(2000), false),
        (
            format!("{}😀x", "中".repeat(1999)),
            format!("{}😀…", "中".repeat(1999)),
            true,
        ),
        ("&amp;".repeat(2000), "&amp;".repeat(2000), false),
        (
            format!("{}&lt;x", "&amp;".repeat(1999)),
            format!("{}&lt;…", "&amp;".repeat(1999)),
            true,
        ),
        (
            format!("{}&#x1F600;x", "a".repeat(1999)),
            format!("{}&#x1F600;…", "a".repeat(1999)),
            true,
        ),
    ];
    for (original, expected, truncated) in cases {
        for already_excerpt in [false, true] {
            let expected = quoted(&expected, truncated || already_excerpt);
            let message = service
                .send(&bot.bot_id, &quoted(&original, already_excerpt))
                .await
                .unwrap()
                .message;
            assert_eq!(message.status, MessageStatus::Normal);
            assert_eq!(message.content, expected);
            assert_eq!(
                service
                    .chat(&bot.bot_id)
                    .await
                    .unwrap()
                    .messages
                    .last()
                    .unwrap()
                    .content,
                expected
            );
            assert_eq!(
                client.inputs.lock().unwrap().last().unwrap().input,
                format!("Source: User\nMessage: {}\n\n{expected}", message.id)
            );
        }
    }
    let capped = quoted(&format!("{}…", "😀".repeat(2000)), true);
    assert_eq!(
        service
            .send(&bot.bot_id, &capped)
            .await
            .unwrap()
            .message
            .content,
        capped
    );
}

#[tokio::test]
async fn file_quotes_and_plain_text_are_unchanged() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    for content in [
        "x".repeat(2100),
        file_quote(&format!("{}.png", "a".repeat(2100))),
        file_quote("chart.png").replace("speaker=\"Bot\"", "speaker=\"User\""),
        format!("Example:\n{}", quoted(&"a".repeat(2100), false)),
        quoted(&"a".repeat(2100), false).replace("speaker=\"Bot\"", "speaker=\"Other\""),
    ] {
        assert_eq!(
            service
                .send(&bot.bot_id, &content)
                .await
                .unwrap()
                .message
                .content,
            content
        );
    }
}

#[tokio::test]
async fn historical_quotes_are_unchanged_when_read_or_retried() {
    let (_root, pool, client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    let content = quoted(&"a".repeat(2100), false);
    sqlx::query("INSERT INTO bot_messages (id, bot_id, kind, content, status) VALUES ('old', ?, 'user_input', ?, 'failed')")
        .bind(&bot.bot_id).bind(&content).execute(&pool).await.unwrap();
    assert_eq!(
        service.chat(&bot.bot_id).await.unwrap().messages[1].content,
        content
    );
    assert_eq!(
        service
            .retry(&bot.bot_id, "old")
            .await
            .unwrap()
            .message
            .content,
        content
    );
    assert_eq!(
        client.inputs.lock().unwrap().last().unwrap().input,
        format!("Source: User\nMessage: old\n\n{content}")
    );
}

#[tokio::test]
async fn saved_user_inputs_link_only_valid_quotes_from_the_same_chat() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let a = service
        .records()
        .create(bot_configuration("A", MODEL), Default::default())
        .await
        .unwrap()
        .bot_id;
    let b = service
        .records()
        .create(bot_configuration("B", MODEL), Default::default())
        .await
        .unwrap()
        .bot_id;
    let target = service.send(&a, "Original").await.unwrap().message;
    for content in [
        quoted("Original", false),
        quoted("Original", true),
        file_quote("chart.png"),
    ] {
        let content = content.replace(MESSAGE_ID, &target.id);
        let input = service.send(&a, &content).await.unwrap().message;
        let context = service.message_context(&a, &input.id).await.unwrap();
        assert_eq!(context.messages, [target.clone(), input]);
        let foreign = service.send(&b, &content).await.unwrap().message;
        assert_eq!(
            service
                .message_context(&b, &foreign.id)
                .await
                .unwrap()
                .messages,
            [foreign]
        );
    }
    for content in [
        quoted("missing", false),
        format!(
            "Example: {}",
            quoted("Original", false).replace(MESSAGE_ID, &target.id)
        ),
        quoted("<nested/>", false).replace(MESSAGE_ID, &target.id),
        quoted("Original", false)
            .replace(MESSAGE_ID, &target.id)
            .replace("message_id=", "file_id="),
        file_quote("chart.png")
            .replace(MESSAGE_ID, &target.id)
            .replace("speaker=\"Bot\"", "speaker=\"User\""),
    ] {
        let input = service.send(&a, &content).await.unwrap().message;
        assert_eq!(
            service
                .message_context(&a, &input.id)
                .await
                .unwrap()
                .messages,
            [input]
        );
    }
}

#[tokio::test]
async fn bot_replies_and_their_links_are_all_or_nothing() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let a = service
        .records()
        .create(bot_configuration("A", MODEL), Default::default())
        .await
        .unwrap()
        .bot_id;
    let b = service
        .records()
        .create(bot_configuration("B", MODEL), Default::default())
        .await
        .unwrap()
        .bot_id;
    let first = service.send(&a, "First").await.unwrap().message;
    let second = service.send(&a, "Second").await.unwrap().message;
    let foreign = service.send(&b, "Other chat").await.unwrap().message;
    let session = service
        .records()
        .get(&a)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let reply = |targets: Vec<String>| ReplyRequest {
        session_id: session.clone(),
        input: Reply {
            message: "Response".into(),
            todo: false,
            resolve_todos: vec![],
            related_to: targets,
        },
    };
    for targets in [
        vec![first.id.clone(), foreign.id],
        vec![first.id.clone(), "missing".into()],
        vec![first.id.clone(); 6],
    ] {
        assert!(service.send_to_user(reply(targets)).await.is_err());
        assert_eq!(
            service.chat(&a).await.unwrap().messages[1..],
            [first.clone(), second.clone()]
        );
    }
    let response = service
        .send_to_user(reply(vec![
            first.id.clone(),
            second.id.clone(),
            first.id.clone(),
        ]))
        .await
        .unwrap()
        .message;
    let followup = service
        .send_to_user(reply(vec![response.id.clone()]))
        .await
        .unwrap()
        .message;
    assert_eq!(
        service
            .message_context(&a, &response.id)
            .await
            .unwrap()
            .messages,
        [first.clone(), second, response.clone(), followup.clone()]
    );
    assert_eq!(
        service
            .message_context(&a, &first.id)
            .await
            .unwrap()
            .messages,
        [first, response, followup]
    );
}
