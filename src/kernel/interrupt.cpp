#include <list>
#include <vector>

#include <assert.h>
#include <stdint.h>
#include <stdio.h>

#include <types/types.h>

#include <kernel/hw/port.hpp>
#include <kernel/hw/timer.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/irq.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>

#define KERNEL_INTERRUPT_GATE_TYPE (0x8e)
#define USER_INTERRUPT_GATE_TYPE (0xee)

constexpr kernel::hw::p8 port_pic1_command{0x20};
constexpr kernel::hw::p8 port_pic1_data{0x21};
constexpr kernel::hw::p8 port_pic2_command{0xa0};
constexpr kernel::hw::p8 port_pic2_data{0xa1};

struct IDT_entry {
    uint16_t offset_low;
    uint16_t segment;

    uint8_t IST;
    uint8_t attributes;

    uint16_t offset_mid;
    uint32_t offset_high;
    uint32_t reserved;
};

static struct IDT_entry IDT[256];

extern "C" uintptr_t ISR_START_ADDR;

SECTION(".text.kinit")
static inline void set_idt_entry(IDT_entry (&idt)[256], int n,
    uintptr_t offset, uint16_t selector, uint8_t type)
{
    idt[n].offset_low = offset & 0xffff;
    idt[n].segment = selector;
    idt[n].IST = 0;
    idt[n].attributes = type;
    idt[n].offset_mid = (offset >> 16) & 0xffff;
    idt[n].offset_high = (offset >> 32) & 0xffffffff;
    idt[n].reserved = 0;
}

using kernel::irq::irq_handler_t;
static std::vector<std::list<irq_handler_t>> s_irq_handlers;

SECTION(".text.kinit")
void kernel::kinit::init_interrupt()
{
    for (int i = 0; i < 0x30; ++i)
        set_idt_entry(IDT, i, ISR_START_ADDR+8*i, 0x08, KERNEL_INTERRUPT_GATE_TYPE);

    uint64_t idt_descriptor[2];
    idt_descriptor[0] = (sizeof(IDT_entry) * 256) << 48;
    idt_descriptor[1] = (uintptr_t)IDT;

    // initialize PIC
    asm volatile("lidt (%0)": :"r"((uintptr_t)idt_descriptor + 6): );
    s_irq_handlers.resize(16);

    // TODO: move this to timer driver
    kernel::irq::register_handler(0, []() {
        kernel::hw::timer::inc_tick();
        schedule();
    });

    port_pic1_command = 0x11; // edge trigger mode
    port_pic1_data = 0x20;    // start from int 0x20
    port_pic1_data = 0x04;    // PIC1 is connected to IRQ2 (1 << 2)
    port_pic1_data = 0x01;    // no buffer mode

    port_pic2_command = 0x11; // edge trigger mode
    port_pic2_data = 0x28;    // start from int 0x28
    port_pic2_data = 0x02;    // connected to IRQ2
    port_pic2_data = 0x01;    // no buffer mode

    // allow all the interrupts
    port_pic1_data = 0x00;
    port_pic2_data = 0x00;
}

void kernel::irq::register_handler(int irqno, irq_handler_t handler)
{
    s_irq_handlers[irqno].emplace_back(std::move(handler));
}

extern "C" void interrupt_handler(
        interrupt_stack_head* context,
        mmx_registers* mmxregs)
{
    // interrupt is a fault
    if (context->int_no < 0x20) {
        auto* with_code = (interrupt_stack_with_code*)context;

        switch (context->int_no) {
        case 6:
        case 8: {
            if (!current_process->attr.system)
                kill_current(SIGSEGV); // noreturn
        } break;
        case 13: {
            if (!current_process->attr.system)
                kill_current(SIGILL); // noreturn
        } break;
        case 14: {
            kernel::mem::paging::handle_page_fault(with_code->error_code);
            context->int_no = (unsigned long)context + 0x80;
        }
        }
        freeze();
    }
    auto* real_context = (interrupt_stack_normal*)context;

    int irqno = context->int_no - 0x20;

    constexpr uint8_t PIC_EOI = 0x20;

    port_pic1_command = PIC_EOI;
    if (irqno >= 8)
        port_pic2_command = PIC_EOI;

    for (const auto& handler : s_irq_handlers[irqno])
        handler();

    if (real_context->cs == 0x1b && current_thread->signals.pending_signal())
        current_thread->signals.handle(real_context, mmxregs);

    context->int_no = (unsigned long)context + 0x78;
    return;
}
