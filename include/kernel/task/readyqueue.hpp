#pragma once

#include <list>

#include <kernel/task/thread.hpp>

namespace kernel::task::dispatcher {

void enqueue(thread* thd);
void dequeue(thread* thd);

void setup_idle(thread* idle_thd);

thread* next();

} // namespace kernel::task
