// Probe: do NSEvent class methods work from this process?
// Prints mouseLocation and pressedMouseButtons every 500 ms for 5 s.
fn main() {
    for _ in 0..10 {
        unsafe {
            let loc: objc2_foundation::NSPoint =
                objc2::msg_send![objc2::class!(NSEvent), mouseLocation];
            let buttons: usize = objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons];
            println!("loc=({},{}) buttons={}", loc.x, loc.y, buttons);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
