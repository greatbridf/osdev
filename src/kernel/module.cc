#include <kernel/module.hpp>

namespace kernel::module {

module::module(const char* name) : name(name) { }

int insmod(module* mod) {
    int ret = mod->init();

    if (ret == MODULE_FAILED) {
        delete mod;
        return MODULE_FAILED;
    }

    return MODULE_SUCCESS;
}

} // namespace kernel::module
