use async_std::{
	io::Error,
	sync::{Arc, Mutex},
	task,
};
use std::io;
use tui::backend::{Backend, CrosstermBackend};
use tui::layout::{Constraint, Direction, Layout};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, Gauge, List, ListItem, Row, Table};
use tui::Terminal;

use crossbeam_channel::Receiver;
use f1_telemetry_client::{
	f1_2020::car::CarStatusData, f1_2020::event::Event, f1_2020::nationality::Nationality,
	f1_2020::packet::Packet2020,
};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DriverDetails {
	driver: String,
	team: String,
	race_number: u8,
	nationality: Nationality,
	name: String,
}

#[derive(Debug, Clone)]
pub struct PositionTable {
	is_player: bool,
	position: u8,
	driver: DriverDetails,
	best_lap: String,
	last_lap: String,
	s1: String,
	s2: String,
	s3: String,
	tyre: String,
	current_lap_num: u8,
}

#[derive(Debug, Clone)]
pub struct CarStatus {
	tyre: String,
	rear_left_tyre: u8,
	rear_right_tyre: u8,
	front_left_tyre: u8,
	front_right_tyre: u8,
}

#[derive(Debug, Clone)]
pub struct PlayerTelemetry {
	speed: u16,
	throttle: f32,
	brake: f32,
	gear: i8,
	suggested_gear: i8,
	engine_rpm: u16,
	drs: bool,
	rev_lights_percent: u8,
}

#[derive(Debug, Clone)]
pub struct AppData {
	// player
	player_index: u8,
	player_details: Option<DriverDetails>,
	player_car_status: Option<CarStatusData>,
	player_telemetry: Option<PlayerTelemetry>,
	positions_table: Vec<PositionTable>,
	participants: Vec<DriverDetails>,
	car_status: Vec<CarStatus>,
	speed_trap: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct App {
	inner: Arc<Mutex<AppData>>,
}

impl App {
	pub fn new() -> Self {
		App {
			inner: Arc::new(Mutex::new(AppData {
				player_index: 255,
				player_details: None,
				player_car_status: None,
				player_telemetry: None,
				positions_table: Vec::with_capacity(22),
				participants: Vec::with_capacity(22),
				car_status: Vec::with_capacity(22),
				speed_trap: None,
			})),
		}
	}

	pub fn start(&mut self, receiver: Receiver<Packet2020>) -> Result<(), Error> {
		let stdout = io::stdout();
		let mut backend = CrosstermBackend::new(stdout);
		backend.clear()?;
		let mut terminal = Terminal::new(backend)?;

		let app_data = Arc::clone(&self.inner);
		task::spawn(async move {
			for msg in receiver {
				let mut data = app_data.lock().await;
				match msg {
					Packet2020::Motion(_motion) => {}
					Packet2020::CarStatus(car_status) => {
						let player_index = car_status.header.player_car_index as usize;

						if let Some(player_car_status) =
						car_status.car_status_data.get(player_index)
						{
							data.player_car_status = Some(player_car_status.clone());
						}

						data.car_status = car_status
						  .clone()
						  .car_status_data
						  .iter()
						  .map(|c| CarStatus {
							  tyre: c.visual_tyre_compound.to_string().to_string(),
							  rear_left_tyre: c.tyres_wear.rear_left,
							  rear_right_tyre: c.tyres_wear.rear_left,
							  front_left_tyre: c.tyres_wear.rear_left,
							  front_right_tyre: c.tyres_wear.rear_left,
						  })
						  .collect();
					}
					Packet2020::CarTelemetry(car_telemetry) => {
						let player_index = car_telemetry.header.player_car_index as usize;
						if let Some(player_telemetry) =
						car_telemetry.car_telemetry_data.get(player_index)
						{
							data.player_telemetry = Some(PlayerTelemetry {
								speed: player_telemetry.speed,
								throttle: player_telemetry.throttle,
								brake: player_telemetry.brake,
								gear: player_telemetry.gear,
								suggested_gear: car_telemetry.suggested_gear,
								engine_rpm: player_telemetry.engine_rpm,
								drs: player_telemetry.drs,
								rev_lights_percent: player_telemetry.rev_lights_percent,
							});
						}
					}
					Packet2020::Participants(participants) => {
						if data.participants.is_empty() {
							let player_index = participants.header.player_car_index as usize;
							let participant_data = participants.participants.clone();

							if let Some(player_details) = participant_data.get(player_index) {
								data.player_details = Some(DriverDetails {
									driver: player_details.driver.name().to_string(),
									team: player_details.team.name().to_string(),
									race_number: player_details.race_number,
									nationality: player_details.nationality,
									name: player_details.name.clone(),
								});
							}

							data.participants = participant_data
							  .iter()
							  .enumerate()
							  .filter(|(_, f)| f.race_number > 0)
							  .map(|(_, p)| DriverDetails {
								  driver: p.driver.name().to_string(),
								  team: p.team.name().to_string(),
								  race_number: p.race_number,
								  nationality: p.nationality,
								  name: p.clone().name,
							  })
							  .collect();
						}
					}
					Packet2020::Lap(lap_data) => {
						let participants = data.participants.clone();
						let car_status = data.car_status.clone();
						if !participants.is_empty() && !car_status.is_empty() {
							let player_index = lap_data.header.player_car_index as usize;
							let lap = lap_data.clone().lap_data;

							data.positions_table = lap
							  .iter()
							  .enumerate()
							  .filter(|(_, f)| f.car_position > 0)
							  .map(|(i, lap)| {
								  let driver = participants.get(i).unwrap();
								  let car = car_status.get(i).unwrap();

								  PositionTable {
									  is_player: i == player_index,
									  position: lap.car_position,
									  driver: DriverDetails {
										  driver: driver.driver.clone(),
										  team: driver.team.clone(),
										  race_number: driver.race_number,
										  nationality: driver.nationality,
										  name: driver.name.clone(),
									  },
									  best_lap: to_lap_time(lap.best_lap_time),
									  last_lap: to_lap_time(lap.last_lap_time),
									  s1: to_lap_time(lap.best_lap_sector_1_time),
									  s2: to_lap_time(lap.best_lap_sector_1_time),
									  s3: to_lap_time(lap.best_lap_sector_1_time),
									  tyre: car.tyre.clone(),
									  current_lap_num: lap.current_lap_num,
								  }
							  })
							  .collect();

							data.positions_table.sort_by_key(|p| p.position);
						}
					}
					Packet2020::Event(event) => {
						let player_index = event.header.player_car_index;
						match event.event {
							Event::SpeedTrap(st) => {
								if st.vehicle_index == player_index {
									data.speed_trap = Some(st.speed);
								}
							}
							_ => {}
						}
					}
					_ => {}
				}

				terminal.autoresize().unwrap();
				terminal
				  .draw(|f| {
					  let chunks = Layout::default()
						.direction(Direction::Horizontal)
						.margin(1)
						.constraints(
							[Constraint::Percentage(70), Constraint::Percentage(30)].as_ref(),
						)
						.split(f.size());

					  let left_block = Block::default().title("Car Data").borders(Borders::ALL);
					  f.render_widget(left_block, chunks[0]);

					  let left_layout = Layout::default()
						.margin(1)
						.constraints(
							[
								Constraint::Percentage(35), // car status
								Constraint::Percentage(10), // rev light
								Constraint::Percentage(10), // rev light
								Constraint::Percentage(10), // rev light
								Constraint::Percentage(35), // car telemetry
							]
							  .as_ref(),
						)
						.split(chunks[0]);

					  let car_status_block =
						Block::default().title("Car Status").borders(Borders::ALL);
					  f.render_widget(car_status_block, left_layout[0]);

					  let car_status = Layout::default()
						.margin(1)
						.direction(Direction::Horizontal)
						.constraints(
							[Constraint::Percentage(50), Constraint::Percentage(50)].as_ref(),
						)
						.split(left_layout[0]);

					  if let Some(car) = &data.player_car_status {
						  let tyres_layout = Layout::default()
							.margin(1)
							.direction(Direction::Vertical)
							.constraints(
								[
									Constraint::Percentage(25),
									Constraint::Percentage(25),
									Constraint::Percentage(25),
									Constraint::Percentage(25),
								]
								  .as_ref(),
							)
							.split(car_status[0]);

						  let tyres_wear_block =
							Block::default().title("Tyres Wear").borders(Borders::NONE);
						  f.render_widget(tyres_wear_block, car_status[0]);

						  let rear_left_color =
							wear_color_percentage(car.tyres_wear.rear_left as usize);
						  let rear_right_color =
							wear_color_percentage(car.tyres_wear.rear_right as usize);
						  let front_left_color =
							wear_color_percentage(car.tyres_wear.front_left as usize);
						  let front_right_color =
							wear_color_percentage(car.tyres_wear.front_right as usize);

						  let rear_left = Gauge::default()
							.block(Block::default().title("Rear Left").borders(Borders::ALL))
							.style(Style::default().fg(Color::White))
							.gauge_style(Style::default().fg(rear_left_color))
							.percent(car.tyres_wear.rear_left as u16);
						  f.render_widget(rear_left, tyres_layout[0]);
						  let rear_right = Gauge::default()
							.block(Block::default().title("Rear Right").borders(Borders::ALL))
							.style(Style::default().fg(Color::White))
							.gauge_style(Style::default().fg(rear_right_color))
							.percent(car.tyres_wear.rear_right as u16);
						  f.render_widget(rear_right, tyres_layout[1]);

						  let front_left = Gauge::default()
							.block(Block::default().title("Front Left").borders(Borders::ALL))
							.style(Style::default().fg(Color::White))
							.gauge_style(Style::default().fg(front_left_color))
							.percent(car.tyres_wear.front_left as u16);
						  f.render_widget(front_left, tyres_layout[2]);
						  let front_right = Gauge::default()
							.block(Block::default().title("Front Right").borders(Borders::ALL))
							.style(Style::default().fg(Color::White))
							.gauge_style(Style::default().fg(front_right_color))
							.percent(car.tyres_wear.front_right as u16);
						  f.render_widget(front_right, tyres_layout[3]);

						  let status_layout = Layout::default()
							.margin(1)
							.direction(Direction::Vertical)
							.constraints([Constraint::Percentage(100)].as_ref())
							.split(car_status[1]);

						  let items = [
							  ListItem::new(format!(
								  "Fuel remaining in laps: {:.2}",
								  car.fuel_remaining_laps
							  ))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("Fuel mix: {}", car.fuel_mix.to_string()))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("Fuel in tank: {:.2}", car.fuel_in_tank))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!(
								  "DRS allowed: {}",
								  car.drs_allowed.to_string()
							  ))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!(
								  "ERS deployment mode: {}",
								  car.ers_deploy_mode.to_string()
							  ))
								.style(Style::default().fg(Color::White)),
						  ];

						  let items = List::new(items)
							.block(Block::default().borders(Borders::ALL).title("Status"));
						  f.render_widget(items, status_layout[0]);
					  };

					  let car_telemetry_layout = Layout::default()
						.direction(Direction::Horizontal)
						.margin(1)
						.constraints(
							[
								Constraint::Percentage(33),
								Constraint::Percentage(33),
								Constraint::Percentage(33),
							]
							  .as_ref(),
						)
						.split(left_layout[4]);

					  if let Some(car_data) = data.player_telemetry.clone() {
						  let rev_light_color =
							wear_color_percentage(car_data.rev_lights_percent as usize);
						  let rev_light = Gauge::default()
							.block(Block::default().title("Rev").borders(Borders::ALL))
							.gauge_style(Style::default().fg(rev_light_color))
							.percent(car_data.rev_lights_percent as u16);
						  f.render_widget(rev_light, left_layout[1]);

						  let brake_color =
							wear_color_percentage((car_data.brake * 100.0).round() as usize);
						  let brake = Gauge::default()
							.block(Block::default().title("Brake").borders(Borders::ALL))
							.gauge_style(Style::default().fg(brake_color))
							.percent((car_data.brake * 100.0).round() as u16);
						  f.render_widget(brake, left_layout[2]);

						  let throttle_color =
							wear_color_percentage((car_data.throttle * 100.0).round() as usize);
						  let throttle = Gauge::default()
							.block(Block::default().title("Throttle").borders(Borders::ALL))
							.gauge_style(Style::default().fg(throttle_color))
							.percent((car_data.throttle * 100.0).round() as u16);
						  f.render_widget(throttle, left_layout[3]);

						  let suggested_gear = if car_data.suggested_gear < 1 {
							  "[N/A]".to_string()
						  } else {
							  car_data.suggested_gear.to_string()
						  };

						  let car_info_list = [
							  ListItem::new(format!("Speed: {} KM/H", car_data.speed))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("Gear: {}", car_data.gear))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("Suggested Gear: {}", suggested_gear))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("DRS: {}", car_data.drs))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("Engine RPM: {}", car_data.engine_rpm))
								.style(Style::default().fg(Color::White)),
							  ListItem::new(format!("Throttle: {}", car_data.throttle))
								.style(Style::default().fg(Color::White)),
						  ];

						  let car_info_list = List::new(car_info_list)
							.block(Block::default().borders(Borders::ALL).title("Car Info"));
						  f.render_widget(car_info_list, car_telemetry_layout[1]);
					  }

					  let right_block =
						Block::default().title("Right Block").borders(Borders::ALL);
					  f.render_widget(right_block, chunks[1]);

					  let right_layout = Layout::default()
						.constraints(
							[Constraint::Percentage(100)].as_ref(),
						)
						.split(chunks[1]);

					  let positions = data.positions_table.iter().map(|p| {
						  let color = if p.is_player {
							  Style::default()
								.fg(Color::White)
								.bg(Color::Magenta)
								.add_modifier(Modifier::BOLD)
						  } else {
							  Style::default().fg(Color::White)
						  };

						  let driver_name = p.driver.driver.clone();
						  let last_name =
							driver_name.split(" ").collect::<Vec<&str>>()[1].to_string();

						  Row::StyledData(
							  vec![
								  p.position.to_string(),
								  last_name,
								  p.current_lap_num.to_string(),
								  p.last_lap.clone(),
								  p.best_lap.clone(),
								  p.tyre.clone(),
							  ]
								.into_iter(),
							  color,
						  )
					  });

					  let live_position = Table::new(
						  ["P", "Driver", "Lap", "Last Lap", "Best Lap", "Tyre"].iter(),
						  positions.clone().into_iter(),
					  )
						.block(
							Block::default()
							  .borders(Borders::ALL)
							  .title("Live Position"),
						)
						.widths(&[
							Constraint::Length(2),
							Constraint::Length(10),
							Constraint::Length(3),
							Constraint::Length(8),
							Constraint::Length(8),
							Constraint::Length(5),
						])
						.style(Style::default().fg(Color::White))
						.column_spacing(5);

					  f.render_widget(live_position, right_layout[0]);
				  })
				  .expect("Error when draw terminal");
			}
		});
		Ok(())
	}
}

fn to_lap_time(lap_time: Duration) -> String {
	let mins = (lap_time.as_secs() / 60) % 60;
	let secs = lap_time.as_secs_f32() % 60.0;
	format!("{}:{:.3}", mins, secs)
}

fn wear_color_percentage(value: usize) -> Color {
	match value {
		0..=50 => Color::Green,
		51..=70 => Color::Yellow,
		_ => Color::Red,
	}
}

#[allow(dead_code)]
fn color_percentage(value: usize) -> Color {
	match value {
		0..=30 => Color::Red,
		31..=70 => Color::Yellow,
		_ => Color::Green,
	}
}
