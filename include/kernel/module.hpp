#pragma once

#include <types/types.h>

#define INTERNAL_MODULE(name, func) \
    SECTION(".kmods") __attribute__((used)) \
    kernel::module::module_loader const name = (func)

namespace kernel::module {

struct module {
    const char* const name;

    explicit module(const char* name);

    virtual ~module() = default;
    module(const module&) = delete;
    module& operator=(const module&) = delete;

    virtual int init() = 0;
};

using module_loader = module* (*)();

constexpr int MODULE_SUCCESS = 0;
constexpr int MODULE_FAILED = 1;
constexpr int MODULE_DELAYED = 2;

// TODO: unique_ptr and Deleter
int insmod(module* mod);

extern "C" module_loader KMOD_LOADERS_START[];

} // namespace kernel::module
