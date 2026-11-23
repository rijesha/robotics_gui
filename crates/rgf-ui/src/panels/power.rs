//! Power usage: battery state, draw over time, and which motors the current goes to.

use crate::widgets::stat;
use crate::{theme, Panel, Station};
use egui_plot::{Line, Plot, PlotPoints};
use rgf_core::codec::str_from;

pub struct PowerPanel;

fn duration(seconds: f32) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{} h {:02} min", s / 3600, s % 3600 / 60)
    } else {
        format!("{} min {:02} s", s / 60, s % 60)
    }
}

impl Panel for PowerPanel {
    fn title(&self) -> &str {
        "Power"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let Some(power) = station.client.power.clone() else {
            ui.colored_label(theme::DIM, "No power telemetry yet.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            stat(ui, "Power", format!("{:.0} W", power.power_w));
            stat(ui, "Voltage", format!("{:.1} V", power.voltage_v));
            stat(ui, "Current", format!("{:.1} A", power.current_a));
            stat(ui, "Used", format!("{:.1} Wh", power.consumed_wh));
        });
        let soc = power.soc_pct / 100.0;
        let color = if soc < 0.15 {
            theme::BAD
        } else if soc < 0.3 {
            theme::WARN
        } else {
            theme::OK
        };
        ui.add(egui::ProgressBar::new(soc).fill(color).text(format!(
            "{:.0}%, about {} left at this rate",
            power.soc_pct,
            duration(power.time_remaining_s)
        )));

        ui.add_space(4.0);
        let now = station.now;
        let points: PlotPoints<'_> = station.power_history.iter().map(|[t, watts, _]| [t - now, *watts]).collect();
        Plot::new("power_history")
            .height(130.0)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .include_y(0.0)
            .x_axis_label("seconds ago")
            .y_axis_label("W")
            .show(ui, |plot| plot.line(Line::new("Power", points).color(theme::ACCENT)));

        ui.add_space(4.0);
        ui.strong("Current by motor");
        let total: f32 = station.client.motors.iter().flatten().map(|m| m.current_a).sum();
        for (i, motor) in station.client.motors.iter().enumerate() {
            let (Some(motor), Some(Some(actuator))) = (motor, station.client.actuators.get(i)) else { continue };
            ui.horizontal(|ui| {
                ui.add_sized([96.0, 16.0], egui::Label::new(str_from(&actuator.name)));
                let share = if total > 0.0 { motor.current_a / total } else { 0.0 };
                ui.add(egui::ProgressBar::new(share).text(format!("{:.1} A", motor.current_a)));
            });
        }
    }
}
