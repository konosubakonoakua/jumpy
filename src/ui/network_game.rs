use bones_framework::networking::online::SearchState;

use crate::prelude::*;

use super::main_menu::MenuPage;

#[derive(Clone, Debug, Default)]
pub enum NetworkGameAction {
    #[default]
    None,
    GoBack,
}

#[derive(Default, Clone)]
pub enum LanMode {
    #[default]
    Join,
    Host {
        service_name: String,
        player_count: usize,
    },
}

#[derive(Eq, PartialEq, Clone)]
pub struct OnlineState {
    player_count: usize,
    matchmaking_server: String,
    search_state: SearchState,
}

impl Default for OnlineState {
    fn default() -> Self {
        Self {
            player_count: 2,
            matchmaking_server: String::new(),
            search_state: default(),
        }
    }
}

#[derive(Clone)]
pub enum MatchKind {
    Lan(LanMode),
    Online(OnlineState),
}

impl Default for MatchKind {
    fn default() -> Self {
        MatchKind::Lan(LanMode::Join)
    }
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum NetworkGameStatus {
    #[default]
    Idle,
    Joining,
    Hosting,
    Searching,
}

#[derive(Clone)]
pub struct NetworkGameState {
    match_kind: MatchKind,
    lan_service_discovery_recv: Option<lan::ServiceDiscoveryReceiver>,
    service_info: Option<lan::ServerInfo>,
    status: NetworkGameStatus,
    joined_players: usize,
    lan_servers: Vec<lan::ServerInfo>,
    ping_update_timer: Timer,
}

impl Default for NetworkGameState {
    fn default() -> Self {
        Self {
            match_kind: default(),
            lan_service_discovery_recv: default(),
            service_info: default(),
            status: default(),
            lan_servers: default(),
            joined_players: default(),
            ping_update_timer: Timer::new(Duration::from_secs(1), TimerMode::Repeating),
        }
    }
}

pub fn network_game_menu(
    world: &World,
    // commands: &mut Commands,
    localization: Localization<GameMeta>,
    meta: Root<GameMeta>,
    ctx: Res<EguiCtx>,
    player_controls: Res<GlobalPlayerControls>,
    storage: ResMut<Storage>,
) //-> NetworkGameAction {
{
    // if player_controls.values().any(|x| x.menu_back_just_pressed) {
    //     return NetworkGameAction::GoBack;
    // }

    let bigger_text_style = &meta.theme.font_styles.bigger;
    let normal_text_style = &meta.theme.font_styles.normal;
    let smaller_text_style = &meta.theme.font_styles.smaller;
    let heading_text_style = &meta.theme.font_styles.heading;
    let normal_button_style = &meta.theme.buttons.normal;
    let small_button_style = &meta.theme.buttons.small;

    // TODO CentralPanel ok?
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(&ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(heading_text_style.size / 4.0);
                ui.label(heading_text_style.rich(localization.get("network-game")));
                ui.label(bigger_text_style.rich(localization.get("configure-match")));
                ui.add_space(heading_text_style.size * 4.0);
            });

            // TODO: set state at end. Is it initialiezd?
            let mut state = ui.ctx().get_state::<NetworkGameState>();


            let available_size = ui.available_size();
            let x_margin = available_size.x / 4.0;
            let outer_margin = egui::style::Margin::symmetric(x_margin, 0.0);

            BorderedFrame::new(&meta.theme.panel.border)
                .margin(outer_margin)
                .padding(meta.theme.panel.padding)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.horizontal(|ui| {
                        // Lan tab
                        let mut lan = egui::RichText::new(localization.get("lan"));
                        if matches!(state.match_kind, MatchKind::Lan(..)) {
                            lan = lan.underline();
                        }
                        if BorderedButton::themed(normal_button_style, lan)
                            .show(ui)
                            .clicked()
                        {
                            state.match_kind = MatchKind::Lan(default());
                        }

                        // Online tab
                        let mut online = egui::RichText::new(localization.get("online"));
                        if matches!(state.match_kind, MatchKind::Online(..)) {
                            online = online.underline();
                        }
                        if BorderedButton::themed(normal_button_style, online)
                            .show(ui)
                            .clicked()
                        {
                            state.match_kind = MatchKind::Online(default());
                        }
                        match &mut state.match_kind {
                            MatchKind::Lan(mode) => {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            // Host tab
                                            let mut host =
                                                egui::RichText::new(localization.get("host"));
                                            if matches!(mode, LanMode::Host { .. }) {
                                                host = host.underline();
                                            }
                                            if BorderedButton::themed(
                                                &meta.theme.buttons.small,
                                                host,
                                            )
                                            .show(ui)
                                            .clicked()
                                            {
                                                *mode = LanMode::Host {
                                                    service_name: localization.get("fish-fight").into_owned(),
                                                    player_count: 2,
                                                };
                                            }

                                            // Join tab
                                            let mut join =
                                                egui::RichText::new(localization.get("join"));
                                            if matches!(mode, LanMode::Join) {
                                                join = join.underline();
                                            }
                                            if BorderedButton::themed(
                                                &meta.theme.buttons.small,
                                                join,
                                            )
                                            .show(ui)
                                            .clicked()
                                            {
                                                *mode = LanMode::Join
                                            }
                                        });
                                    },
                                );
                            },
                            MatchKind::Online(_online_state) => {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            normal_text_style.rich(localization.get("search-for-match"))
                                        );
                                    },
                                );
                            }
                        }
                    });

                    let NetworkGameState {
                        match_kind,
                        lan_service_discovery_recv,
                        lan_servers,
                        service_info: host_info,
                        status,
                        ping_update_timer,
                        joined_players,
                    } = &mut state;

                    ui.separator();
                    ui.add_space(normal_text_style.size);

                    match match_kind {
                        // LAN game
                        MatchKind::Lan(mode) => match mode {
                            LanMode::Join => {
                                // Stop any running server
                                if let Some(service_info) = host_info.take() {
                                    lan::stop_server(&service_info);
                                    *status = NetworkGameStatus::Idle;
                                }
                                lan::prepare_to_join(lan_servers, lan_service_discovery_recv, ping_update_timer);

                                if *status != NetworkGameStatus::Joining {
                                    ui.label(
                                        normal_text_style.rich(localization.get("servers"))
                                    );
                                    ui.add_space(normal_text_style.size / 2.0);

                                    ui.indent("servers", |ui| {
                                        for server in lan_servers.iter() {
                                            ui.horizontal(|ui| {
                                                if BorderedButton::themed(
                                                    &meta.theme.buttons.normal,
                                                    server.service.get_hostname().trim_end_matches('.'),
                                                )
                                                .min_size(vec2(ui.available_width() * 0.8, 0.0))
                                                .show(ui)
                                                .clicked()
                                                {
                                                    lan::join_server(server);
                                                    *status = NetworkGameStatus::Joining;
                                                }

                                                let label_text = egui::RichText::new(format!(
                                                    "🖧 {}ms",
                                                    server
                                                        .ping
                                                        .map(|x| x.to_string())
                                                        .unwrap_or("?".into())
                                                ))
                                                .size(normal_text_style.size);
                                                ui.label(label_text)
                                            });
                                        }

                                        if lan_servers.is_empty() {
                                            ui.label(
                                                normal_text_style.rich(
                                                localization.get("no-servers")),
                                            );
                                        }
                                    });

                                // If we are trying to join a match.
                                } else {
                                    ui.label(
                                        normal_text_style.rich(
                                        localization.get("joining"))
                                    );

                                    if let Some(lan_socket) = lan::wait_game_start() {
                                        world.resources.insert(lan_socket);
                                        *status = default();
                                       ui.ctx().set_state(MenuPage::PlayerSelect);
                                    }
                                }

                                ui.add_space(normal_text_style.size / 2.0);
                            }
                            LanMode::Host {
                                service_name,
                                player_count,
                            } => {
                                ui.scope(|ui| {
                                    ui.set_enabled(*status != NetworkGameStatus::Hosting);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            normal_text_style.rich(
                                            localization.get("server-name"))
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(service_name)
                                                .font(normal_text_style.id()),
                                        );
                                        *service_name = service_name.replace(' ', "-");
                                    });
                                    ui.add_space(normal_text_style.size / 2.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            normal_text_style.rich(localization.get("player-count"))
                                        );
                                        ui.add_space(normal_text_style.size);
                                        ui.scope(|ui| {
                                            ui.set_enabled(*player_count > 2);
                                            if BorderedButton::themed(small_button_style, "-")
                                                .min_size(vec2(normal_text_style.size * 2.0, 0.0))
                                                .show(ui)
                                                .clicked()
                                            {
                                                *player_count = player_count
                                                    .saturating_sub(1)
                                                    .clamp(2, MAX_PLAYERS);
                                            }
                                        });
                                        ui.label(normal_text_style.rich(player_count.to_string()));
                                        ui.scope(|ui| {
                                            ui.set_enabled(*player_count < MAX_PLAYERS);
                                            if BorderedButton::themed(small_button_style, "+")
                                          .min_size(vec2(normal_text_style.size * 2.0, 0.0))
                                                .show(ui)
                                                .clicked()
                                            {
                                                *player_count = player_count
                                                    .saturating_add(1)
                                                    .clamp(2, MAX_PLAYERS);
                                            }
                                        });

                                        *service_name = service_name.replace(' ', "-");
                                    });
                                });

                                let (is_recreated, service_info) = lan::prepare_to_host(host_info, service_name);
                                if is_recreated {
                                    *status = NetworkGameStatus::Idle;
                                }

                                ui.add_space(meta.theme.font_styles.normal.size);

                                if *status == NetworkGameStatus::Idle {
                                    if BorderedButton::themed(
                                        small_button_style,
                                        localization.get("start-server"),
                                    )
                                    .show(ui)
                                    .clicked()
                                    {
                                        *status = NetworkGameStatus::Hosting;
                                        lan::start_server(service_info.clone(), *player_count);
                                    }

                                // If we are hosting a match currently
                                } else if *status == NetworkGameStatus::Hosting {
                                    if let Some(socket) = lan::wait_players(joined_players, service_info) {
                                        world.resources.insert(socket);
                                        *status = default();
                                        ui.ctx().set_state(MenuPage::PlayerSelect);
                                    }

                                    ui.horizontal(|ui| {
                                        if BorderedButton::themed(
                                            small_button_style,
                                            localization.get("stop-server"),
                                        )
                                        .show(ui)
                                        .clicked()
                                        {
                                            lan::stop_server(service_info);
                                            *status = NetworkGameStatus::Idle;
                                        }

                                        ui.label(
                                            normal_text_style.rich(
                                            format!(
                                                "{} {} / {}",
                                                localization.get("players"),
                                                *joined_players + 1, // Add one to count the host.
                                                player_count
                                            ))
                                        );
                                    });
                                }
                            }
                        }

                        // Online game
                        MatchKind::Online(OnlineState {
                            player_count,
                            matchmaking_server,
                            mut search_state,
                        }) => {
                            // Get the matchmaking server from the settings.
                            if matchmaking_server.is_empty() {
                                *matchmaking_server =
                                    storage
                                    .get::<Settings>()
                                    .unwrap_or_else(|| &meta.default_settings)
                                    .matchmaking_server.clone();
                            }

                            ui.horizontal(|ui| {
                                ui.set_enabled(*status == NetworkGameStatus::Idle);
                                ui.label(
                                    normal_text_style.rich(localization.get("player-count"))
                                );

                                ui.scope(|ui| {
                                    ui.set_enabled(*player_count > 2);
                                    if BorderedButton::themed(small_button_style, "-")
                                        .min_size(vec2(normal_text_style.size * 2.0, 0.0))
                                        .show(ui)
                                        .clicked()
                                    {
                                        *player_count =
                                            player_count.saturating_sub(1).clamp(2, MAX_PLAYERS);
                                    }
                                });
                                ui.label(normal_text_style.rich(player_count.to_string()));
                                ui.scope(|ui| {
                                    ui.set_enabled(*player_count < MAX_PLAYERS);
                                    if BorderedButton::themed(small_button_style, "+")
                                        .min_size(vec2(normal_text_style.size * 2.0, 0.0))
                                        .show(ui)
                                        .clicked()
                                    {
                                        *player_count =
                                            player_count.saturating_add(1).clamp(2, MAX_PLAYERS);
                                    }
                                });
                            });

                            ui.add_space(normal_text_style.size);

                            if *status == NetworkGameStatus::Idle {
                                if BorderedButton::themed(
                                    small_button_style,
                                    localization.get("search"),
                                )
                                .show(ui)
                                .clicked()
                                {
                                    *status = NetworkGameStatus::Searching;
                                    online::start_search_for_game(matchmaking_server.clone(), *player_count);
                                }
                            } else if *status == NetworkGameStatus::Searching {
                                if let Some(online_socket) = online::update_search_for_game(&mut search_state) {
                                    world.resources.insert(online_socket);

                                    if search_state == SearchState::Connecting {
                                        *status = NetworkGameStatus::default();
                                        ui.ctx().set_state(MenuPage::PlayerSelect);
                                    }
                                }

                                ui.horizontal(|ui| {
                                    if BorderedButton::themed(
                                        small_button_style,
                                        localization.get("cancel"),
                                    )
                                    .show(ui)
                                    .clicked()
                                    {
                                        if let Err(err) = online::stop_search_for_game() {
                                            error!("Failed to stop searching for online game: {err}");
                                        }
                                        search_state = default();
                                        *status = NetworkGameStatus::Idle;
                                    }

                                    ui.label(
                                        smaller_text_style.rich(
                                        match search_state {
                                            SearchState::Connecting => {
                                                localization.get("connecting").to_string()
                                            }
                                            SearchState::Searching => {
                                                localization.get("searching").to_string()
                                            }
                                            SearchState::WaitingForPlayers(current) => {
                                                localization.get(&format!(
                                                    "waiting-for-players?current={current}&total={player_count}",
                                                )).to_string()
                                            }
                                        }),
                                    );
                                });
                            }
                        }
                    }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if BorderedButton::themed(normal_button_style, localization.get("back"))
                        .show(ui)
                        .clicked()
                        || player_controls.values().any(|x| x.menu_back_just_pressed)
                    {
                        match status {
                            NetworkGameStatus::Idle => (),
                            NetworkGameStatus::Searching => {
                                if let Err(err) = online::stop_search_for_game() {
                                    error!("Error stopping search: {:?}", err);
                                }

                                *status = NetworkGameStatus::Idle;
                            }
                            NetworkGameStatus::Joining => {
                                lan::leave_server();
                                *status = NetworkGameStatus::Idle;
                            }
                            NetworkGameStatus::Hosting => {
                                *status = NetworkGameStatus::Idle;
                                if let Some(service_info) = host_info.take() {
                                    lan::stop_server(&service_info);
                                }
                            }
                        }

                        ui.ctx().set_state(MenuPage::Home);

                    }
             });


            // Update state after modifications
            ui.ctx().set_state(state);

        });
    });
}
