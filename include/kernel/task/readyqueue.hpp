#pragma once

#include <list>

#include <types/lock.hpp>

#include <kernel/task/thread.hpp>

namespace kernel::task::dispatcher {

void enqueue(thread* thd);
void dequeue(thread* thd);

thread* next();

} // namespace kernel::task
