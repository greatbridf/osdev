#pragma once

#include <memory>

#include <types/types.h>

#define MODULE_LOADER(name) \
    static std::unique_ptr<kernel::kmod::kmod> __module##name##_loader()

#define INTERNAL_MODULE(name, type)                                         \
    MODULE_LOADER(name);                                                    \
    SECTION(".kmods")                                                       \
    __attribute__((used))                                                   \
    std::unique_ptr<kernel::kmod::kmod> (*const __module##name##_entry)() = \
        __module##name##_loader;                                            \
    MODULE_LOADER(name) {                                                   \
        return std::make_unique<type>();                                    \
    }

namespace kernel::kmod {

struct kmod {
    const char* const name;

    explicit kmod(const char* name);

    virtual ~kmod() = default;
    kmod(const kmod&) = delete;
    kmod& operator=(const kmod&) = delete;

    virtual int init() = 0;
};

extern "C" std::unique_ptr<kmod> (*const KMOD_LOADERS_START[])();
void load_internal_modules();

} // namespace kernel::kmod
