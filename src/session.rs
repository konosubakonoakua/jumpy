//! Session management for matches.
//!
//! The [`SessionManager`] is used to create, stop, snapshot, and restore game matches. A session
//! refers to an in-progress game match.
//!
//! Right now there are two kinds of sessions: local sessions and network sessions. These are
//! implemented by the [`LocalSessionRunner`] and
//! [`GgrsSessionRunner`][crate::networking::GgrsSessionRunner] types respectively.
//!
//! Both of them implmenent [`SessionRunner`] which is a trait used by the [`SessionManager`] to
//! advance the game simulation properly.

use bevy::utils::Instant;
use downcast_rs::{impl_downcast, Downcast};
use jumpy_core::input::{PlayerControl, PlayerInputs};

use crate::{main_menu::MenuPage, prelude::*};

/// Session plugin.
pub struct JumpySessionPlugin;

/// Stage label for the game session stages
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
#[system_set(base)]
pub enum SessionStage {
    /// Update the game session.
    Update,
}

impl Plugin for JumpySessionPlugin {
    fn build(&self, app: &mut App) {
        let mut session_schedule = Schedule::new();
        session_schedule.add_systems((
            ensure_2_players,
            collect_local_input.pipe(update_game),
            play_sounds,
        ));

        app.add_plugin(bones_bevy_renderer::BonesRendererPlugin::<Session>::with_sync_time(false))
            .add_plugin(jumpy_core::metadata::JumpyCoreAssetsPlugin)
            .init_resource::<CurrentEditorInput>()
            .configure_set(
                SessionStage::Update
                    .before(CoreSet::Update)
                    .run_if(in_state(InGameState::Playing))
                    .run_if(in_state(EngineState::InGame))
                    .run_if(resource_exists::<Session>()),
            )
            .add_system(move |world: &mut World| {
                let in_correct_state = {
                    world.resource::<State<EngineState>>().0 == EngineState::InGame
                        && world.resource::<State<InGameState>>().0 == InGameState::Playing
                };

                if !in_correct_state || !world.contains_resource::<Session>() {
                    return;
                }

                loop {
                    let should_run =
                        world.resource_scope(|world: &mut World, mut session: Mut<Session>| {
                            session.run_criteria(world.resource::<Time>())
                        });

                    match should_run {
                        ShouldRun::Yes => {
                            session_schedule.run(world);
                            break;
                        }
                        ShouldRun::No => break,
                        ShouldRun::YesAndCheckAgain => {
                            session_schedule.run(world);
                        }
                    }
                }
            });
    }
}

/// A resource containing the in-progress game session.
#[derive(Resource, Deref, DerefMut)]
pub struct Session(pub Box<dyn SessionRunner>);

/// Trait implemented by types that know how to advance the core game simulation.
///
/// Things like fixed frame updates are expected to be handled by the session runner.
///
/// The [`GgrsSessionRunner`][crate::networking::GgrsSessionRunner] is an example of how a custom
/// runner can be used for running a network game.
pub trait SessionRunner: Sync + Send + Downcast {
    /// Get mutable access to the [`CoreSession`].
    fn core_session(&mut self) -> &mut CoreSession;
    /// Get mutable access to the [`bones::World`] from in the [`CoreSession`].
    fn world(&mut self) -> &mut bones::World {
        &mut self.core_session().world
    }
    /// Restart the session.
    fn restart(&mut self);
    /// Get the control input for the player with the given `player_idx`.
    fn get_player_input(&mut self, player_idx: usize) -> PlayerControl {
        self.core_session()
            .update_input(|inputs| inputs.players[player_idx].control.clone())
    }
    /// Set the player input for the player with the given `player_idx`.
    fn set_player_input(&mut self, player_idx: usize, control: PlayerControl);
    /// Advance the game simmulation.
    fn advance(&mut self, bevy_world: &mut World) -> Result<(), SessionError>;
    /// Return whether or not the simulation should run, given the current time.
    ///
    /// This is used to created fixed refresh rates.
    fn run_criteria(&mut self, time: &Time) -> ShouldRun;
    /// Returns the player index of the player if we are in a network game.
    ///
    /// In a network game, we currently only allow for one local player, so this allows the session
    /// to find out which player we are playing as so it can map the local player 1's input to the
    /// appropriate network player.
    fn network_player_idx(&mut self) -> Option<usize>;
}
impl_downcast!(SessionRunner);

/// Possible errors returned by [`SessionRunner::advance`].
pub enum SessionError {
    /// The session was disconnected.
    Disconnected,
}

/// Implementation of [`SessionRunner`] for local games.
///
/// This is almost as simple as a [`SessionRunner`] can get: it just advances the game simulation at
/// the fixed [`jumpy_core::FPS`].
pub struct LocalSessionRunner {
    pub core: CoreSession,
    pub accumulator: f64,
    pub loop_start: Option<Instant>,
}

impl LocalSessionRunner {
    fn new(core: CoreSession) -> Self
    where
        Self: Sized,
    {
        LocalSessionRunner {
            core,
            accumulator: default(),
            loop_start: default(),
        }
    }
}

/// Indicates whether or not a session advance should be run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShouldRun {
    Yes,
    No,
    YesAndCheckAgain,
}

impl SessionRunner for LocalSessionRunner {
    fn core_session(&mut self) -> &mut CoreSession {
        &mut self.core
    }

    fn set_player_input(&mut self, player_idx: usize, control: PlayerControl) {
        self.core.update_input(|inputs| {
            inputs.players[player_idx].control = control;
        });
    }

    fn restart(&mut self) {
        self.core.restart();
    }

    fn advance(&mut self, bevy_world: &mut World) -> Result<(), SessionError> {
        self.core.advance(bevy_world);

        Ok(())
    }
    fn run_criteria(&mut self, time: &Time) -> ShouldRun {
        const STEP: f64 = 1.0 / jumpy_core::FPS as f64;
        let delta = time.delta_seconds_f64();
        if self.loop_start.is_none() {
            self.accumulator += delta;
        }

        if self.accumulator >= STEP {
            let start = self.loop_start.get_or_insert_with(Instant::now);

            let loop_too_long = (Instant::now() - *start).as_secs_f64() > STEP;

            if loop_too_long {
                warn!("Frame took too long: couldn't keep up with fixed update.");
                self.accumulator = 0.0;
                self.loop_start = None;
                ShouldRun::No
            } else {
                self.accumulator -= STEP;
                ShouldRun::YesAndCheckAgain
            }
        } else {
            self.loop_start = None;
            ShouldRun::No
        }
    }
    fn network_player_idx(&mut self) -> Option<usize> {
        None
    }
}

// Give bones_bevy_render plugin access to the bones world in our game session.
impl bones_bevy_renderer::HasBonesWorld for Session {
    fn world(&mut self) -> &mut bones::World {
        self.0.world()
    }
}

/// Helper for creating and stopping game sessions.
#[derive(SystemParam)]
pub struct SessionManager<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub menu_camera: Query<'w, 's, &'static mut Camera, With<MenuCamera>>,
    pub session: Option<ResMut<'w, Session>>,
    pub core_meta_arc: Res<'w, CoreMetaArc>,
}

impl<'w, 's> SessionManager<'w, 's> {
    /// Start a game session
    pub fn start_local(&mut self, info: CoreSessionInfo) {
        let session = Session(Box::new(LocalSessionRunner::new(CoreSession::new(info))));
        self.commands.insert_resource(session);
        self.menu_camera.for_each_mut(|mut x| x.is_active = false);
    }

    /// Start a network game session.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_network(
        &mut self,
        core_info: CoreSessionInfo,
        ggrs_info: crate::networking::GgrsSessionRunnerInfo,
    ) {
        let session = Session(Box::new(crate::networking::GgrsSessionRunner::new(
            CoreSession::new(core_info),
            ggrs_info,
        )));
        self.commands.insert_resource(session);
        self.menu_camera.for_each_mut(|mut x| x.is_active = false);
        self.commands
            .insert_resource(NextState(Some(InGameState::Playing)));
        self.commands
            .insert_resource(NextState(Some(EngineState::InGame)));
    }

    /// Restart a game session without changing the settings
    pub fn restart(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.restart();
        }
    }

    /// Stop a game session
    pub fn stop(&mut self) {
        self.commands.remove_resource::<Session>();
        self.menu_camera.for_each_mut(|mut x| x.is_active = true);
    }
}

/// Helper system to make sure there are two players on the board, if ever the game is in the middle
/// of playing and there are no players on the board.
///
/// This is primarily for the editor, which may be started without going through the player
/// selection screen.
fn ensure_2_players(mut session: ResMut<Session>, core_meta: Res<CoreMetaArc>) {
    let player_inputs = session
        .world()
        .resource::<jumpy_core::input::PlayerInputs>();
    let mut player_inputs = player_inputs.borrow_mut();

    if player_inputs.players.iter().all(|x| !x.active) {
        for i in 0..2 {
            player_inputs.players[i].active = true;
            player_inputs.players[i].selected_player = core_meta.players[i].clone();
        }
    }
}

/// Update the input to the game session.
fn collect_local_input(
    mut session: ResMut<Session>,
    player_input_collectors: Query<(&PlayerInputCollector, &ActionState<PlayerAction>)>,
    mut current_editor_input: ResMut<CurrentEditorInput>,
) {
    let network_player_idx = session.network_player_idx();

    if let Some(local_session) = session.downcast_mut::<LocalSessionRunner>() {
        // TODO: Handle editor input for non-local sessions.
        let editor_input = current_editor_input.take();
        local_session.core.update_input(|inputs| {
            inputs.players[0].editor_input = editor_input;
        });
    }

    for (player_idx, action_state) in &player_input_collectors {
        let is_ai = {
            let world = &session.core_session().world;
            let inputs = world.resource::<PlayerInputs>();
            let inputs = inputs.borrow();
            inputs.players[player_idx.0].is_ai
        };
        if (player_idx.0 != 0 && network_player_idx.is_some()) || is_ai {
            continue;
        }

        let mut control = session.0.get_player_input(player_idx.0);

        let jump_pressed = action_state.pressed(PlayerAction::Jump);
        control.jump_just_pressed = jump_pressed && !control.jump_pressed;
        control.jump_pressed = jump_pressed;

        let grab_pressed = action_state.pressed(PlayerAction::Grab);
        control.grab_just_pressed = grab_pressed && !control.grab_pressed;
        control.grab_pressed = grab_pressed;

        let shoot_pressed = action_state.pressed(PlayerAction::Shoot);
        control.shoot_just_pressed = shoot_pressed && !control.shoot_pressed;
        control.shoot_pressed = shoot_pressed;

        let was_moving = control.move_direction.length_squared() > f32::MIN_POSITIVE;
        control.move_direction = action_state.axis_pair(PlayerAction::Move).unwrap().xy();
        let is_moving = control.move_direction.length_squared() > f32::MIN_POSITIVE;
        control.just_moved = !was_moving && is_moving;

        session.set_player_input(network_player_idx.unwrap_or(player_idx.0), control);
    }
}

/// Update the game session simulation.
fn update_game(world: &mut World) {
    let mut session = world.remove_resource::<Session>().unwrap();

    // Advance the game session
    if let Err(e) = session.advance(world) {
        match e {
            SessionError::Disconnected => {
                error!("Network session disconnected");
                // Don't return the session to the world

                // Go back to the menu
                let mut cameras = world.query_filtered::<&mut Camera, With<MenuCamera>>();
                cameras.for_each_mut(world, |mut camera| camera.is_active = true);
                world.insert_resource(MenuPage::Home);
                world.insert_resource(NextState(Some(EngineState::MainMenu)));
                world.insert_resource(NextState(Some(InGameState::Playing)));
            }
        }

    // If the session is OK
    } else {
        // Return the session to the world
        world.insert_resource(session);
    }
}

/// Play sounds from the game session.
pub fn play_sounds(audio: Res<AudioChannel<EffectsChannel>>, mut session: ResMut<Session>) {
    // Get the sound queue out of the world
    let queue = session
        .world()
        .run_initialized_system(move |mut audio_events: bones::ResMut<bones::AudioEvents>| {
            Ok(audio_events.queue.drain(..).collect::<Vec<_>>())
        })
        .unwrap();

    // Play all the sounds in the queue
    for event in queue {
        match event {
            bones::AudioEvent::PlaySound {
                sound_source,
                volume,
            } => {
                audio
                    .play(sound_source.get_bevy_handle_untyped().typed())
                    .with_volume(volume);
            }
        }
    }
}
