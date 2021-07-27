#include <asm/port_io.h>
#include <kernel/event/event.h>
#include <kernel/hw/keyboard.h>
#include <kernel/input/input_event.h>

extern "C" void
handle_keyboard_interrupt(void)
{
    input_event evt {
        .type = input_event::input_event_type::KEYBOARD,
        .code = KEY_DOWN,
        .data = 0
    };

    uint8_t keycode = asm_inb(PORT_KEYDATA);
    if (keycode >= 0xd8) {
        // TODO: report not_supported event
        return;
    }

    // key release
    if (keycode >= 0x80) {
        evt.code = KEY_UP;
        keycode -= 0x80;
    }

    evt.data = keycode;

    commit_input_event(&evt);
}
