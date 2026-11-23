//! Power usage: battery state, draw over time, and which motors the current goes to.

use crate::widgets::{card, empty_state, meter, stats};
use crate::{theme, Panel, Station};
use egui_plot::{Line, Plot, PlotPoints};
use rgf_core::codec::str_from;

pub struct PowerPanel;

fn duration(seconds: f32) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{} h {:02} min", s / 3600, s % 3600 / 60)
    } else {
        format!("{} min", s / 60)
    }
}

impl Panel for PowerPanel {
    fn title(&self) -> &str {
        "Power"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let p = theme::of(ui);
        let Some(power) = station.client.power.clone() else {
            empty_state(ui, "No power data yet", "Waiting for battery telemetry from the vehicle.");
            return;
        };
        card(ui, "Battery", |ui| {
            let soc = power.soc_pct / 100.0;
            let color = if soc < 0.15 {
                p.bad
            } else if soc < 0.3 {
                p.warn
            } else {
                p.ok
            };
            let text = format!("{:.0}%   about {} left at this rate", power.soc_pct, duration(power.time_remaining_s));
            meter(ui, soc, theme::mix(p.raised, color, 0.75), text, 22.0);
            ui.add_space(4.0);
            stats(
                ui,
                &[
                    ("Power", format!("{:.0}", power.power_w), "W"),
                    ("Voltage", format!("{:.1}", power.voltage_v), "V"),
                    ("Current", format!("{:.1}", power.current_a), "A"),
                    ("Used", format!("{:.1}", power.consumed_wh), "Wh"),
                ],
            );
        });
        ui.add_space(4.0);

        card(ui, "Power draw", |ui| {
            let now = station.now;
            let points: PlotPoints<'_> = station.power_history.iter().map(|[t, watts, _]| [t - now, *watts]).collect();
            Plot::new("power_history")
                .height(120.0)
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .show_axes([true, true])
                .include_y(0.0)
                .x_axis_label("seconds ago")
                .y_axis_label("W")
                .show(ui, |plot| plot.line(Line::new("Power", points).color(p.accent).width(2.0)));
        });
        ui.add_space(4.0);

        card(ui, "Current by motor", |ui| {
            let peak = station.client.motors.iter().flatten().map(|m| m.current_a).fold(0.1f32, f32::max);
            egui::Grid::new("current_by_motor").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                for (i, motor) in station.client.motors.iter().enumerate() {
                    let (Some(motor), Some(Some(actuator))) = (motor, station.client.actuators.get(i)) else {
                        continue;
                    };
                    ui.label(str_from(&actuator.name));
                    meter(
                        ui,
                        motor.current_a / peak,
                        theme::mix(p.raised, p.accent, 0.7),
                        format!("{:.1} A", motor.current_a),
                        16.0,
                    );
                    ui.end_row();
                }
            });
        });
    }
}
