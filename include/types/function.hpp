#pragma once
#include <types/cplusplus.hpp>

namespace std {

namespace __inner {

    template <typename Ret, typename... Args>
    class _function_base {
    public:
        constexpr _function_base() = default;
        virtual constexpr ~_function_base() = default;
        virtual constexpr Ret operator()(Args&&... args) const = 0;
    };

    template <typename FuncLike, typename Ret, typename... Args>
    class _function : public _function_base<Ret, Args...> {
    private:
        FuncLike func;

    public:
        constexpr _function(FuncLike&& _func)
            : func(types::forward<FuncLike>(_func))
        {
        }
        constexpr ~_function() = default;

        constexpr Ret operator()(Args&&... args) const override
        {
            return func(types::forward<Args>(args)...);
        }
    };

} // namespace __inner

template <typename>
class function;

template <typename Ret, typename... Args>
class function<Ret(Args...)> {
private:
    char _data[sizeof(void*) * 2];
    using fb_t = __inner::_function_base<Ret, Args...>;
    constexpr fb_t* _f(void) const
    {
        return (fb_t*)_data;
    }

public:
    template <typename FuncLike>
    constexpr function(FuncLike&& func)
    {
        static_assert(sizeof(FuncLike) <= sizeof(_data));
        new (_f()) __inner::_function<FuncLike, Ret, Args...>(types::forward<FuncLike>(func));
    }

    template <typename FuncPtr>
    constexpr function(FuncPtr* funcPtr)
    {
        new (_f()) __inner::_function<typename types::traits::decay<FuncPtr>::type, Ret, Args...>(
            types::forward<typename types::traits::decay<FuncPtr>::type>(funcPtr));
    }

    constexpr ~function()
    {
        _f()->~_function_base();
    }

    constexpr Ret operator()(Args... args) const
    {
        return (*_f())(types::forward<Args>(args)...);
    }
};

} // namespace std
