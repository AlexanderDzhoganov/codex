//! Keep the user's selected model when automatic usage-limit switching is disabled.

use super::*;
use crate::chatwidget::UserMessage;
use crate::chatwidget::tests::helpers::normalize_snapshot_paths;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn disabled_automatic_model_switching_preserves_settings_and_submitted_model() -> Result<()> {
    for reserve in [false, true] {
        for mode_kind in [ModeKind::Default, ModeKind::Plan] {
            let (mut app, mut events, _ops) = make_test_app_with_channels().await;
            let (mut server, requests, proxy) =
                backend_banner_fallback_tests::start_fallback_thread(&mut app).await?;
            luna_reserve_recovery_tests::configure_reserve_catalog(&mut app);
            app.chat_widget
                .set_feature_enabled(Feature::AutomaticModelSwitching, /*enabled*/ false);
            app.chat_widget
                .set_reasoning_effort(Some(ReasoningEffortConfig::High));
            if mode_kind == ModeKind::Plan {
                app.chat_widget
                    .handle_key_event(KeyEvent::from(KeyCode::BackTab));
            }
            let expected_mode = app.chat_widget.effective_collaboration_mode();
            while events.try_recv().is_ok() {}
            requests.lock().unwrap().clear();
            let response = if reserve {
                luna_reserve_recovery_tests::reserve_response()
            } else {
                backend_banner_fallback_tests::fallback_response()
            };
            let hard_stop_generation = app.rate_limit_hard_stop_generation;
            let mut tui = crate::tui::test_support::make_test_tui()?;
            app.handle_event(
                &mut tui,
                &mut server,
                AppEvent::RateLimitsLoaded {
                    request_id: 1,
                    origin: RateLimitRefreshOrigin::StatusCommand { request_id: 0 },
                    hard_stop_generation,
                    result: Ok(response),
                },
            )
            .await?;

            assert_eq!(
                app.chat_widget.effective_collaboration_mode(),
                expected_mode
            );
            assert!(requests.lock().unwrap().is_empty());
            if mode_kind == ModeKind::Default {
                insta::assert_snapshot!(
                    if reserve {
                        "reserve_with_automatic_model_switching_disabled"
                    } else {
                        "model_limit_with_automatic_model_switching_disabled"
                    },
                    normalize_snapshot_paths(render_bottom_popup(
                        &app.chat_widget,
                        /*width*/ 80
                    ))
                );
            }
            app.chat_widget
                .handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            app.chat_widget
                .restore_user_message_to_composer(UserMessage::from("continue"));
            app.chat_widget
                .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            let submitted = std::iter::from_fn(|| events.try_recv().ok())
                .find(|event| matches!(event, AppEvent::CodexOp(AppCommand::UserTurn { .. })))
                .expect("submitted user turn");
            app.handle_event(&mut tui, &mut server, submitted).await?;
            let sent = requests.lock().unwrap().clone();
            let methods: Vec<_> = sent.iter().map(|request| request.method.as_str()).collect();
            assert_eq!(methods, ["turn/start"]);
            let turn: codex_app_server_protocol::TurnStartParams =
                serde_json::from_value(sent[0].params.clone().unwrap())?;
            assert_eq!(
                (turn.model, turn.effort, turn.collaboration_mode),
                (
                    Some(expected_mode.model().to_string()),
                    expected_mode.reasoning_effort(),
                    Some(expected_mode),
                )
            );
            server.shutdown().await?;
            proxy.await??;
        }
    }
    Ok(())
}

#[tokio::test]
async fn disabled_automatic_model_switching_preserves_resumed_reserve_model() -> Result<()> {
    let (mut app, _events, _ops) = make_test_app_with_channels().await;
    let (mut server, requests, proxy) =
        backend_banner_fallback_tests::start_fallback_thread(&mut app).await?;
    luna_reserve_recovery_tests::configure_reserve_catalog(&mut app);
    app.chat_widget
        .update_backend_banner(&luna_reserve_recovery_tests::reserve_response());
    app.apply_backend_banner_fallback(&mut server).await;
    let expected_mode = app.chat_widget.effective_collaboration_mode();
    assert_eq!(expected_mode.model(), "gpt-reserve");
    let mut resumed = app.primary_session_configured.clone().unwrap();
    resumed.model = "gpt-reserve".into();
    resumed.collaboration_mode = Some(Box::new(expected_mode.clone()));
    app.config
        .features
        .disable(Feature::AutomaticModelSwitching)?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    let mut init = app.chatwidget_init_for_forked_or_resumed_thread(
        &mut tui,
        app.config.clone(),
        /*initial_user_message*/ None,
    );
    init.has_chatgpt_account = true;
    init.has_codex_backend_auth = true;
    app.replace_chat_widget(ChatWidget::new_with_app_event(init));
    app.chat_widget.handle_thread_session(resumed);
    requests.lock().unwrap().clear();
    let mut recovered = luna_reserve_recovery_tests::reserve_response();
    recovered.ordinary_usage_allowed = Some(true);
    recovered.rate_limit_upsell = None;
    app.chat_widget.update_backend_banner(&recovered);
    app.apply_backend_banner_fallback(&mut server).await;

    assert_eq!(
        app.chat_widget.effective_collaboration_mode(),
        expected_mode
    );
    assert!(requests.lock().unwrap().is_empty());
    server.shutdown().await?;
    proxy.await??;
    Ok(())
}
