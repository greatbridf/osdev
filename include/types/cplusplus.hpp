#pragma once

#ifdef __cplusplus
#include <type_traits>

namespace types {

class non_copyable {
   public:
    virtual ~non_copyable() = default;
    non_copyable() = default;
    non_copyable(const non_copyable&) = delete;
    non_copyable& operator=(const non_copyable&) = delete;
};

} // namespace types

#endif
