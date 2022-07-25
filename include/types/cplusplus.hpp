#pragma once

#include <types/stdint.h>

#ifdef __cplusplus

namespace types {

template <typename T, T _value>
struct constant_value {
    static constexpr T value = _value;
};
using true_type = constant_value<bool, true>;
using false_type = constant_value<bool, false>;

};

namespace types::traits::inner {

template <typename Tp, typename>
struct remove_pointer {
    using type = Tp;
};

template <typename Tp, typename T>
struct remove_pointer<Tp, T*> {
    using type = T;
};

template <typename Tr, typename>
struct remove_reference {
    using type = Tr;
};

template <typename Tr, typename T>
struct remove_reference<Tr, T&> {
    using type = T;
};

} // namespace types::traits::inner

namespace types::traits {

template <typename Tp>
struct remove_pointer
    : inner::remove_pointer<Tp, Tp> {
};

template <typename Tr>
struct remove_reference
    : inner::remove_reference<Tr, Tr> {
};

template <typename T>
struct add_pointer {
    using type = T*;
};

template <typename T>
struct add_reference {
    using type = T&;
};

template <typename T>
struct remove_cv {
    using type = T;
};
template <typename T>
struct remove_cv<const T> {
    using type = T;
};
template <typename T>
struct remove_cv<volatile T> {
    using type = T;
};
template <typename T>
struct remove_cv<const volatile T> {
    using type = T;
};

template <typename T>
struct is_pointer : false_type {
};

template <typename T>
struct is_pointer<T*> : true_type {
};

template <typename T>
struct decay {
private:
    using U = remove_reference<T>;

public:
    using type = typename remove_cv<U>::type;
};

} // namespace types::traits

namespace types {
template <typename T>
constexpr T&& move(T& val)
{
    return static_cast<T&&>(val);
}
template <typename T>
constexpr T&& forward(typename traits::remove_reference<T>::type& val)
{
    return static_cast<T&&>(val);
}

template <typename>
struct template_true_type : public true_type {
};
template <typename>
struct template_false_type : public false_type {
};

template <typename, typename>
struct is_same : false_type {
};

template <typename T>
struct is_same<T, T> : true_type {
};

template <typename T>
struct add_rvalue_reference {
    using type = T&&;
};
template <>
struct add_rvalue_reference<void> {
    using type = void;
};

template <typename Src, typename Dst>
concept convertible_to = (traits::is_pointer<Src>::value && is_same<Dst, uint32_t>::value)
    || (traits::is_pointer<Dst>::value && is_same<Src, uint32_t>::value)
    || requires(Src _src)
{
    { static_cast<Dst>(_src) };
};

template <typename T>
concept PointerType = traits::is_pointer<T>::value;

template <typename A, typename B>
concept same_as = is_same<A, B>::value;

} // namespace types

#endif
