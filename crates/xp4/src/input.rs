use crate::{Execution, Xp4Session};
use serde::Deserialize;
use serde_json::{Value, json};
pub const BUTTONS: [u32; 6] = [0x9f, 0xa0, 0xa1, 0xa2, 0x0f, 0x11];

fn stopped(state: &Value) -> bool {
    !state["cpu"]["failure"].is_null()
        || !state["cpu"]["abort_failure"].is_null()
        || !state["error"].is_null()
        || !state["guestHalt"].is_null()
}
fn snapshot(cpu: &Xp4Session, progress: Execution) -> Value {
    let mut state = cpu.state();
    state["error"] = json!(progress.error);
    state
}
fn scan(cpu: &Xp4Session, input: u32, pressed: bool) -> Result<Value, String> {
    // Menu mode handoffs can defer scanner debounce beyond 32M instructions.
    for _ in 0..1600 {
        let mut state = cpu.execute_batch(250_000)?;
        if state.stopped() {
            return Ok(snapshot(cpu, state));
        }
        let sample = cpu.virtual_button_sample(input).ok();
        let Some(sample) = sample.filter(|b| b.len() == 8 && b[1] <= 1 && b[2] <= 1) else {
            state.error = Some("虚拟按键的固件采样不可用；未推断其电平。".into());
            return Ok(snapshot(cpu, state));
        };
        if sample[1] == u8::from(pressed) && sample[2] == u8::from(pressed) {
            return Ok(snapshot(cpu, state));
        }
    }
    let mut state = cpu.state();
    state["error"] = json!("固件尚未确认按键采样，已达到本次执行预算。");
    Ok(state)
}
pub fn press_button(cpu: &Xp4Session, input: u32) -> Result<Value, String> {
    let state = cpu.state();
    if stopped(&state) {
        return Ok(state);
    }
    for input in BUTTONS {
        cpu.set_virtual_button(input, false)?;
    }
    let state = scan(cpu, input, false)?;
    if stopped(&state) {
        return Ok(state);
    }
    cpu.set_virtual_button(input, true)?;
    let pressed = scan(cpu, input, true);
    cpu.set_virtual_button(input, false)?;
    let state = pressed?;
    if stopped(&state) {
        let mut fresh = cpu.state();
        fresh["error"] = state["error"].clone();
        return Ok(fresh);
    }
    let state = scan(cpu, input, false)?;
    if stopped(&state) {
        return Ok(state);
    }
    cpu.execute(1_000_000)
}

#[derive(Deserialize)]
struct Dial {
    channels: [usize; 2],
    clockwise: [[u16; 2]; 4],
    #[serde(rename = "holdInstructions")]
    hold: u32,
}
fn dial_phase(dial: &Dial, inputs: &[Value], direction: i32) -> Result<[u16; 2], String> {
    if direction != 1 && direction != -1 {
        return Err("Invalid command dial direction.".into());
    }
    let mut levels = [0; 2];
    for (i, channel) in dial.channels.iter().enumerate() {
        levels[i] = inputs
            .get(*channel)
            .and_then(Value::as_u64)
            .filter(|v| *v <= 4095)
            .ok_or("Command dial ADC inputs are unavailable.")?;
    }
    let index = dial
        .clockwise
        .iter()
        .position(|pair| {
            pair.iter()
                .zip(levels)
                .all(|(a, b)| (*a > 2048) == (b > 2048))
        })
        .map_or(-1, |i| i as i32);
    Ok(dial.clockwise[(index + direction + 4) as usize % 4])
}
pub fn rotate_dial(cpu: &Xp4Session, id: &str, direction: i32) -> Result<Value, String> {
    if direction != 1 && direction != -1 {
        return Err("Invalid command dial direction.".into());
    }
    let mut state = cpu.state();
    if stopped(&state) {
        return Ok(state);
    }
    let measured: Value =
        serde_json::from_str(include_str!("../../../profiles/measured/xt4-controls.json")).unwrap();
    let dial: Dial = serde_json::from_value(
        measured["encoders"]
            .get(id)
            .ok_or("Invalid command dial.")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let diagnostics = cpu.diagnostics()?;
    let inputs = diagnostics["devices"]["xp4-adc"]["inputs"]
        .as_array()
        .ok_or("Command dial ADC inputs are unavailable.")?;
    let values = dial_phase(&dial, inputs, direction)?;
    for (channel, value) in dial.channels.into_iter().zip(values) {
        cpu.set_adc(channel, value)?;
    }
    let mut progress = cpu.run_state();
    for _ in (0..dial.hold).step_by(1_000_000) {
        progress = cpu.execute_batch(1_000_000)?;
        if progress.stopped() {
            break;
        }
    }
    state = snapshot(cpu, progress);
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dial_preserves_all_restored_phases_in_both_directions() {
        let measured: Value =
            serde_json::from_str(include_str!("../../../profiles/measured/xt4-controls.json"))
                .unwrap();
        for value in measured["encoders"].as_object().unwrap().values() {
            let dial: Dial = serde_json::from_value(value.clone()).unwrap();
            for start in 0..4 {
                let mut inputs = vec![json!(2048); 12];
                for (channel, level) in dial.channels.into_iter().zip(dial.clockwise[start]) {
                    inputs[channel] = json!(level);
                }
                for direction in [-1, 1] {
                    assert_eq!(
                        dial_phase(&dial, &inputs, direction).unwrap(),
                        dial.clockwise[(start as i32 + direction + 4) as usize % 4]
                    );
                }
                assert!(dial_phase(&dial, &inputs, 0).is_err());
                inputs[dial.channels[0]] = json!(4096);
                assert!(dial_phase(&dial, &inputs, 1).is_err());
            }
        }
    }
}
