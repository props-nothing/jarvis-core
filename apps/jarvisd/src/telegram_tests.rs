//! Tests for the Telegram chat, against a stand-in for the Bot API that follows the documented contract
//! (`docs/research/integrations/telegram.md`): `POST /bot<token>/<method>`, JSON in, `{"ok":..,"result":..}` out.
//!
//! The stand-in proves the daemon's side. It cannot prove Telegram's: that needs a real bot, and `P11-005` says so.

use std::collections::VecDeque;

use axum::{Router, extract::Path as UrlPath, routing::post};
use jarvis_core::ClientCredential;

use super::*;

const TOKEN: &str = "123456:TESTTOKEN-not-a-real-one";

/// What the stand-in remembers: the updates still to hand out, and every call it received.
#[derive(Default)]
struct Fake {
    updates: Mutex<VecDeque<Value>>,
    calls: Mutex<Vec<(String, String, Value)>>,
    next_message: Mutex<i64>,
    reject_token: Mutex<bool>,
    /// Answer a message that asks for HTML the way Telegram answers markup it cannot parse.
    reject_html: Mutex<bool>,
}

impl Fake {
    fn push(&self, update: Value) {
        self.updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(update);
    }

    fn calls_to(&self, method: &str) -> Vec<Value> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, called, _)| called == method)
            .map(|(_, _, body)| body.clone())
            .collect()
    }

    fn texts_to(&self, chat: i64) -> Vec<String> {
        self.calls_to("sendMessage")
            .into_iter()
            .filter(|body| body["chat_id"].as_i64() == Some(chat))
            .filter_map(|body| body["text"].as_str().map(str::to_owned))
            .collect()
    }

    fn saw_token_in_path(&self) -> bool {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|(bot, _, _)| bot == &format!("bot{TOKEN}"))
    }
}

async fn serve(fake: Arc<Fake>) -> String {
    async fn handle(
        axum::extract::State(fake): axum::extract::State<Arc<Fake>>,
        UrlPath((bot, method)): UrlPath<(String, String)>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        fake.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((bot, method.clone(), body.clone()));
        let ok = |result: Value| Json(json!({ "ok": true, "result": result }));
        match method.as_str() {
            "getMe"
                if *fake
                    .reject_token
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) =>
            {
                Json(json!({ "ok": false, "error_code": 401, "description": "Unauthorized" }))
            }
            "getMe" => ok(json!({ "id": 1, "is_bot": true, "username": "jarvis_test_bot" })),
            "getUpdates" => {
                let offset = body["offset"].as_i64().unwrap_or(0);
                for _ in 0..10 {
                    let due: Vec<Value> = {
                        let mut queue = fake.updates.lock().unwrap_or_else(PoisonError::into_inner);
                        queue.retain(|update| update["update_id"].as_i64().unwrap_or(0) >= offset);
                        queue.iter().cloned().collect()
                    };
                    if !due.is_empty() {
                        return ok(Value::Array(due));
                    }
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
                ok(json!([]))
            }
            "sendMessage"
                if body.get("parse_mode").is_some()
                    && *fake
                        .reject_html
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) =>
            {
                Json(
                    json!({ "ok": false, "error_code": 400, "description": "Bad Request: can't parse entities: Unsupported start tag" }),
                )
            }
            "sendMessage" => {
                let mut next = fake
                    .next_message
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                *next += 1;
                ok(json!({ "message_id": *next }))
            }
            _ => ok(json!(true)),
        }
    }
    let app = Router::new()
        .route("/{bot}/{method}", post(handle))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("bind the stand-in: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("{error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}")
}

struct Profile(std::path::PathBuf);

impl Drop for Profile {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

struct Rig {
    _profile: Profile,
    fake: Arc<Fake>,
    bridge: Arc<TelegramBridge>,
    state: GatewayState,
    access: Arc<crate::mcp_access::McpAccess>,
    base: String,
    paths: AppPaths,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn rig() -> Rig {
    let directory =
        std::env::temp_dir().join(format!("jarvis-telegram-{}", jarvis_core::scratch_tag()));
    std::fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
    let profile = Profile(directory.clone());
    let paths = AppPaths::from_root(&directory).unwrap_or_else(|error| panic!("paths: {error}"));
    std::fs::create_dir_all(paths.config()).unwrap_or_else(|error| panic!("{error}"));
    let token_file = paths.config().join("telegram.key");
    std::fs::write(&token_file, TOKEN).unwrap_or_else(|error| panic!("{error}"));
    let model_key = paths.config().join("model.key");
    std::fs::write(&model_key, "ollama").unwrap_or_else(|error| panic!("{error}"));
    let document = format!(
        "schema_version = 1\n\n[profile]\nname = \"default\"\n\n[logging]\nlevel = \"info\"\n\n[daemon]\nshutdown_timeout_seconds = 30\nhttp_enabled = true\nexecutor_model = \"openai-compatible\"\nexecutor_model_name = \"m1\"\nexecutor_base_url = 'http://localhost:11434/v1'\nexecutor_api_key_ref = '{}'\ntelegram_token_ref = '{}'\n",
        model_key.display(),
        token_file.display()
    );
    std::fs::write(
        jarvis_storage::ConfigStore::from_paths(&paths).path(),
        document,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let database = Arc::new(
        jarvis_storage::SqliteDatabase::open(
            &directory.join(jarvis_storage::DEFAULT_DATABASE_FILENAME),
        )
        .await
        .unwrap_or_else(|error| panic!("open the database: {error}")),
    );
    let fake = Arc::new(Fake::default());
    let base = serve(Arc::clone(&fake)).await;
    let mut bridge = TelegramBridge::with_base(paths.clone(), base.clone());
    bridge.watch_every = Duration::from_millis(80);
    let bridge = Arc::new(bridge);
    let access = Arc::new(crate::mcp_access::McpAccess::open(paths.config()));
    let credential = ClientCredential::generate().unwrap_or_else(|error| panic!("{error}"));
    let state = GatewayState::new(database, credential)
        .with_mcp_access(Arc::clone(&access))
        .with_telegram(Arc::clone(&bridge));
    let task = tokio::spawn(Arc::clone(&bridge).run(state.clone()));
    Rig {
        _profile: profile,
        fake,
        bridge,
        state,
        access,
        base,
        paths,
        task,
    }
}

fn message(update_id: i64, chat: i64, name: &str, text: &str) -> Value {
    json!({
        "update_id": update_id,
        "message": {
            "message_id": update_id,
            "from": { "id": chat, "is_bot": false, "first_name": name },
            "chat": { "id": chat, "type": "private" },
            "text": text,
        }
    })
}

fn tap(update_id: i64, from: i64, data: &str) -> Value {
    json!({
        "update_id": update_id,
        "callback_query": {
            "id": format!("cb{update_id}"),
            "from": { "id": from, "is_bot": false, "first_name": "x" },
            "message": { "message_id": 77, "chat": { "id": from, "type": "private" }, "text": "card" },
            "data": data,
        }
    })
}

async fn until(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn code_in(texts: &[String]) -> Option<String> {
    texts.iter().find_map(|text| {
        text.split(|c: char| !c.is_ascii_digit())
            .find(|word| word.len() == 6)
            .map(str::to_owned)
    })
}

/// **A stranger is told a code, only the code proves the owner, and afterwards everyone else gets silence.**
#[tokio::test]
async fn pairing_needs_the_code_and_then_only_that_chat_is_answered() {
    let rig = rig().await;
    rig.fake.push(message(1, 111, "Jan", "hello"));
    until("a code to be sent", || {
        code_in(&rig.fake.texts_to(111)).is_some()
    })
    .await;
    let code = code_in(&rig.fake.texts_to(111)).unwrap_or_default();
    assert_eq!(rig.bridge.status()["state"], "waiting_for_code");
    assert_eq!(rig.bridge.status()["bot"], "jarvis_test_bot");

    // A second person writing while the first holds a live code is not told anything.
    rig.fake.push(message(2, 222, "Mallory", "let me in"));
    // A wrong code is refused and counted; the right one pairs.
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert_eq!(rig.bridge.pair(wrong).await, Err(PairError::Wrong));
    assert_eq!(rig.bridge.pair(&code).await, Ok("Jan".to_owned()));
    assert_eq!(rig.bridge.status()["state"], "paired");
    assert_eq!(rig.bridge.status()["paired_as"], "Jan");
    until("the welcome", || {
        rig.fake
            .texts_to(111)
            .iter()
            .any(|text| text.starts_with("Connected"))
    })
    .await;
    assert!(
        rig.fake.texts_to(222).is_empty(),
        "a second writer must get no reply"
    );

    // Once paired, a stranger is ignored without a word, and the owner can ask for help.
    rig.fake.push(message(3, 222, "Mallory", "hello again"));
    rig.fake.push(message(4, 111, "Jan", "/help"));
    until("the help text", || {
        rig.fake
            .texts_to(111)
            .iter()
            .any(|text| text.contains("Approve and Deny"))
    })
    .await;
    assert!(
        rig.fake.texts_to(222).is_empty(),
        "a stranger must never be answered"
    );
    assert!(
        rig.fake.saw_token_in_path() && !rig.bridge.status().to_string().contains(TOKEN),
        "the token is used on the wire and never appears in what Settings shows"
    );

    // The pairing survives a restart, and unpairing forgets it.
    let reborn = TelegramBridge::with_base(rig.paths.clone(), rig.base.clone());
    assert_eq!(
        reborn.lock().paired.as_ref().map(|paired| paired.chat_id),
        Some(111)
    );
    reborn.unpair();
    assert!(
        TelegramBridge::with_base(rig.paths.clone(), rig.base.clone())
            .lock()
            .paired
            .is_none()
    );
}

/// **A code is good for five wrong tries, and a code nobody asked for does not exist.**
#[tokio::test]
async fn a_code_survives_only_five_wrong_tries() {
    let rig = rig().await;
    assert_eq!(rig.bridge.pair("123456").await, Err(PairError::NoCode));
    rig.fake.push(message(1, 111, "Jan", "hi"));
    until("a code", || code_in(&rig.fake.texts_to(111)).is_some()).await;
    let code = code_in(&rig.fake.texts_to(111)).unwrap_or_default();
    let wrong = if code == "999999" { "999998" } else { "999999" };
    for _ in 0..4 {
        assert_eq!(rig.bridge.pair(wrong).await, Err(PairError::Wrong));
    }
    assert_eq!(rig.bridge.pair(wrong).await, Err(PairError::TooManyTries));
    assert_eq!(
        rig.bridge.pair(&code).await,
        Err(PairError::NoCode),
        "the right code is no good once the tries ran out"
    );
    assert_eq!(rig.bridge.status()["state"], "waiting_for_message");
}

async fn pair_jan(rig: &Rig) {
    rig.fake.push(message(1, 111, "Jan", "hi"));
    until("a code", || code_in(&rig.fake.texts_to(111)).is_some()).await;
    let code = code_in(&rig.fake.texts_to(111)).unwrap_or_default();
    assert!(rig.bridge.pair(&code).await.is_ok());
}

/// **An approval reaches the phone as a card with two buttons, one tap answers it, and a stranger's tap does nothing.**
#[tokio::test]
async fn an_approval_is_a_card_and_one_tap_decides_it() {
    let rig = rig().await;
    pair_jan(&rig).await;
    // Let the watcher make its first pass, so what comes next is new.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let approvals = Arc::clone(rig.access.approvals());
    let (id, answer) = approvals
        .ask(
            "mcp.browser.browser_click",
            "1.0.0",
            3,
            "Click the Buy button",
            &json!({ "element": "Buy" }),
            Duration::from_secs(60),
        )
        .unwrap_or_else(|error| panic!("ask: {error:?}"));
    until("the card", || {
        rig.fake
            .texts_to(111)
            .iter()
            .any(|text| text.contains("mcp.browser.browser_click") && text.contains("Buy"))
    })
    .await;
    let card = rig
        .fake
        .calls_to("sendMessage")
        .into_iter()
        .find(|body| body["reply_markup"].is_object())
        .unwrap_or_else(|| panic!("the card must carry buttons"));
    let buttons = card["reply_markup"]["inline_keyboard"][0].clone();
    assert_eq!(buttons[0]["callback_data"], format!("a:{id}"));
    assert_eq!(buttons[1]["callback_data"], format!("d:{id}"));

    // A stranger's tap is acknowledged and changes nothing.
    rig.fake.push(tap(10, 222, &format!("a:{id}")));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        approvals.list().len(),
        1,
        "a stranger must not decide anything"
    );

    rig.fake.push(tap(11, 111, &format!("a:{id}")));
    let approved = tokio::time::timeout(Duration::from_secs(5), answer)
        .await
        .unwrap_or_else(|_| panic!("the call was never released"))
        .unwrap_or(false);
    assert!(approved);
    until("the card to be retired", || {
        rig.fake.calls_to("editMessageText").iter().any(|body| {
            body["text"]
                .as_str()
                .is_some_and(|text| text.ends_with("Approved."))
        })
    })
    .await;
    assert!(approvals.list().is_empty());
    // The watcher notices the approval is gone; it must leave the card saying what the tap decided.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        rig.fake
            .calls_to("editMessageText")
            .iter()
            .all(|body| !body["text"]
                .as_str()
                .unwrap_or_default()
                .ends_with("Answered.")),
        "the card must not be overwritten"
    );
}

/// **Deny refuses the call, and a tap on an answered card is told so rather than failing.**
#[tokio::test]
async fn deny_refuses_and_a_late_tap_is_told_it_is_over() {
    let rig = rig().await;
    pair_jan(&rig).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (id, answer) = rig
        .access
        .approvals()
        .ask(
            "mcp.x.y",
            "1.0.0",
            3,
            "Do it",
            &json!({}),
            Duration::from_secs(60),
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
    until("the card", || {
        rig.fake
            .calls_to("sendMessage")
            .iter()
            .any(|body| body["reply_markup"].is_object())
    })
    .await;
    rig.fake.push(tap(20, 111, &format!("d:{id}")));
    assert!(!answer.await.unwrap_or(true));
    rig.fake.push(tap(21, 111, &format!("a:{id}")));
    until("the late answer", || {
        rig.fake
            .calls_to("answerCallbackQuery")
            .iter()
            .any(|body| body["text"] == "That is no longer waiting")
    })
    .await;
}

/// **A token Telegram rejects is reported without the token, and removing it puts Telegram off.**
#[tokio::test]
async fn a_rejected_token_is_reported_and_never_echoed() {
    let rig = rig().await;
    *rig.fake
        .reject_token
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = true;
    until("the error state", || {
        rig.bridge.status()["state"] == "error"
    })
    .await;
    let shown = rig.bridge.status().to_string();
    assert!(
        !shown.contains(TOKEN) && !shown.contains("TESTTOKEN"),
        "{shown}"
    );
    assert!(shown.contains("did not accept"), "{shown}");
}

/// **A scheduled result reaches the paired chat, and nothing is sent when no chat is paired.**
#[tokio::test]
async fn a_result_is_told_to_the_paired_chat_only() {
    let rig = rig().await;
    rig.bridge.tell("nobody is paired").await;
    assert!(rig.fake.calls_to("sendMessage").is_empty());
    pair_jan(&rig).await;
    rig.bridge.tell("Your morning briefing is ready.").await;
    assert!(
        rig.fake
            .texts_to(111)
            .iter()
            .any(|text| text.contains("morning briefing"))
    );
    let _ = &rig.state;
}

/// **An answer in Markdown arrives formatted for Telegram: the tags are sent with `parse_mode` HTML.**
#[tokio::test]
async fn an_answer_is_sent_as_formatted_html() {
    let rig = rig().await;
    pair_jan(&rig).await;
    rig.bridge
        .tell("## Klaar\n\nIk vond **drie** bedrijven:\n\n- Rabobank\n- Vopak\n\n| A | B |\n|---|---|\n| 1 | 2 |")
        .await;
    let sent = rig
        .fake
        .calls_to("sendMessage")
        .into_iter()
        .find(|body| {
            body["text"]
                .as_str()
                .is_some_and(|text| text.contains("Klaar"))
        })
        .unwrap_or_else(|| panic!("the answer was not sent"));
    assert_eq!(sent["parse_mode"], "HTML");
    let text = sent["text"].as_str().unwrap_or_default();
    for wanted in ["<b>Klaar</b>", "<b>drie</b>", "\u{2022} Rabobank", "<pre>A"] {
        assert!(text.contains(wanted), "{wanted:?} missing from {text}");
    }
    assert!(!text.contains("**") && !text.contains("## "), "{text}");
}

/// **A message Telegram refuses to parse is sent again as plain text, so nothing is lost.**
#[tokio::test]
async fn a_message_telegram_will_not_parse_is_resent_as_plain_text() {
    let rig = rig().await;
    pair_jan(&rig).await;
    *rig.fake
        .reject_html
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = true;
    rig.bridge.tell("Even **vet** en <b>tag</b>.").await;
    let sent: Vec<Value> = rig
        .fake
        .calls_to("sendMessage")
        .into_iter()
        .filter(|body| {
            body["text"]
                .as_str()
                .is_some_and(|text| text.contains("vet"))
        })
        .collect();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[0]["parse_mode"], "HTML");
    assert!(sent[1].get("parse_mode").is_none());
    assert_eq!(sent[1]["text"], "Even vet en <b>tag</b>.");
}

/// **Text with more than a message's worth is sent as several formatted messages, each within Telegram's limit.**
#[tokio::test]
async fn a_long_answer_is_sent_as_several_messages_within_the_limit() {
    let rig = rig().await;
    pair_jan(&rig).await;
    let paragraph = "woord ".repeat(300);
    rig.bridge
        .tell(&format!(
            "{paragraph}\n\n{paragraph}\n\n{paragraph}\n\n**einde**"
        ))
        .await;
    let texts: Vec<String> = rig
        .fake
        .calls_to("sendMessage")
        .into_iter()
        .filter(|body| body["parse_mode"] == "HTML")
        .filter_map(|body| body["text"].as_str().map(str::to_owned))
        .collect();
    assert!(texts.len() >= 2, "{}", texts.len());
    assert!(
        texts
            .iter()
            .all(|text| crate::telegram_format::strip_tags(text).chars().count() <= 4096)
    );
    assert!(
        texts
            .last()
            .is_some_and(|text| text.ends_with("<b>einde</b>"))
    );
}
#[test]
fn a_code_is_six_digits() {
    for _ in 0..50 {
        let code = new_code();
        assert_eq!(code.len(), 6, "{code}");
        assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
    }
}

#[test]
fn an_approval_card_says_what_it_will_do_and_clips_long_arguments() {
    let approval = jarvis_protocol::PendingApprovalReply {
        approval_id: "id".to_owned(),
        state: "pending".to_owned(),
        run_id: String::new(),
        call_id: "id".to_owned(),
        tool: "jarvis.gmail.send".to_owned(),
        tool_version: "1.0.0".to_owned(),
        risk_level: 3,
        preview: "Send an email".to_owned(),
        arguments: Some(json!({ "body": "x".repeat(2000) })),
        created_at: jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
        expires_at: jarvis_core::UtcTimestamp::now(&jarvis_core::SystemClock),
    };
    let card = approval_card(&approval);
    assert!(card.starts_with("JARVIS wants to use jarvis.gmail.send (risk 3)."));
    assert!(card.contains("Send an email") && card.ends_with("..."));
    assert!(card.chars().count() < 900);
    let hidden = jarvis_protocol::PendingApprovalReply {
        arguments: None,
        ..approval
    };
    assert!(approval_card(&hidden).contains("(arguments not shown)"));
}
