#pragma once

#include <list>

#include <kernel/task/thread.hpp>

namespace kernel::task::dispatcher {

void enqueue(thread* thd);
void dequeue(thread* thd);

thread* next();

} // namespace kernel::task
