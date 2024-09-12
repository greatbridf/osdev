#include <map>

#include <assert.h>

#include <kernel/log.hpp>
#include <kernel/module.hpp>

namespace kernel::kmod {

kmod::kmod(const char* name) : name(name) {}

static std::map<std::string, std::unique_ptr<kmod>> modules;

void load_internal_modules() {
    for (auto loader = KMOD_LOADERS_START; *loader; ++loader) {
        auto mod = (*loader)();
        if (!mod)
            continue;

        if (int ret = mod->init(); ret != 0) {
            kmsgf("[kernel] An error(%x) occured while loading \"%s\"", ret,
                  mod->name);
            continue;
        }

        auto [_, inserted] = modules.try_emplace(mod->name, std::move(mod));
        assert(inserted);
    }
}

} // namespace kernel::kmod
