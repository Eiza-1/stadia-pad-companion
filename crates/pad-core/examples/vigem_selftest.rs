//! Hardware-free check of the virtual pad: submit known states and read them back through XInput.

use pad_core::mapping::XOut;
use pad_core::win::vigem::VirtualPad;
use windows::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};

fn read_all() -> Vec<(u32, (u16, i16, i16, u8))> {
    (0..4u32)
        .filter_map(|i| {
            let mut s = XINPUT_STATE::default();
            (unsafe { XInputGetState(i, &mut s) } == 0)
                .then(|| (i, (s.Gamepad.wButtons.0, s.Gamepad.sThumbLX, s.Gamepad.sThumbLY, s.Gamepad.bLeftTrigger)))
        })
        .collect()
}

fn main() {
    println!("before: {:?}", read_all());
    let mut pad = VirtualPad::connect(|l, s| println!("rumble {l} {s}")).expect("vigem");
    std::thread::sleep(std::time::Duration::from_millis(300));
    println!("fresh:  {:?}", read_all());
    let ok = pad.submit(&XOut::default());
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("neutral submit ok={ok}: {:?}", read_all());
    let ok = pad.submit(&XOut { buttons: 0x1000, lt: 77, lx: 12345, ly: -2222, ..Default::default() });
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("A+lx12345 submit ok={ok}: {:?}", read_all());
    let ok = pad.submit(&XOut::default());
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("back to zero ok={ok}: {:?}", read_all());
    drop(pad);
    std::thread::sleep(std::time::Duration::from_millis(300));
    println!("after drop: {:?}", read_all());
}
