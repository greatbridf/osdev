#pragma once

#include <stdint.h>

#ifdef __cplusplus

namespace types {

template <typename T, T _value>
struct constant_value {
    static constexpr T value = _value;
};
using true_type = constant_value<bool, true>;
using false_type = constant_value<bool, false>;

};

namespace types::traits {

template <bool Expression, typename TTrue, typename TFalse>
struct condition {
    using type = TFalse;
};
template <typename TTrue, typename TFalse>
struct condition<true, TTrue, TFalse> {
    using type = TTrue;
};

template <typename T>
struct remove_pointer {
    using type = T;
};
template <typename T>
struct remove_pointer<T*> {
    using type = T;
};
template <typename T>
struct remove_reference {
    using type = T;
};
template <typename T>
struct remove_reference<T&> {
    using type = T;
};
template <typename T>
struct remove_reference<T&&> {
    using type = T;
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
struct add_const {
    using type = const T;
};
template <typename T>
struct add_const<const T> {
    using type = const T;
};
template <>
struct add_const<void> {
    using type = void;
};

template <typename T>
struct is_pointer : false_type {
};

template <typename T>
struct is_pointer<T*> : true_type {
};

template <typename T>
struct is_const : public false_type {
};
template <typename T>
struct is_const<const T> : public true_type {
};

template <typename U, template <typename...> class T, typename...>
struct is_template_instance : public false_type {
};
template <template <typename...> class T, typename... Ts>
struct is_template_instance<T<Ts...>, T> : public true_type {
};

template <typename T>
struct decay {
private:
    using U = typename remove_reference<T>::type;

public:
    using type = typename remove_cv<U>::type;
};

} // namespace types::traits

namespace types {
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

class non_copyable {
public:
    non_copyable() = default;
    non_copyable(const non_copyable&) = delete;
    non_copyable& operator=(const non_copyable&) = delete;
};

} // namespace types

#endif
