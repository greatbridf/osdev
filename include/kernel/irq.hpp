#pragma once

#include <functional>

namespace kernel::irq {

using irq_handler_t = std::function<void()>;

void register_handler(int irqno, irq_handler_t handler);

};
