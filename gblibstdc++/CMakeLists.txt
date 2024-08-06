cmake_minimum_required(VERSION 3.15)
project(gblibstdc++)

set(CMAKE_CXX_STANDARD 20)

add_library(gblibstdc++ STATIC src/stdc++.cpp)

set(GBSTDLIBCPP_PUBLIC_HEADERS
    gblibstdc++/include
    gblibstdc++/include/bit
    gblibstdc++/include/memory
    gblibstdc++/include/tuple
    gblibstdc++/include/type_traits
    gblibstdc++/include/algorithm
    gblibstdc++/include/cstddef
    gblibstdc++/include/map
    gblibstdc++/include/bits
    gblibstdc++/include/bits/compressed_pair
    gblibstdc++/include/bits/fwd_functional
    gblibstdc++/include/bits/rbtree
    gblibstdc++/include/bits/iter_ops
    gblibstdc++/include/bits/type_traits
    gblibstdc++/include/bits/type_traits/is_es
    gblibstdc++/include/bits/type_traits/remove_add
    gblibstdc++/include/bits/type_traits/base
    gblibstdc++/include/bits/tuple_tools
    gblibstdc++/include/bits/sequence
    gblibstdc++/include/bits/forward
    gblibstdc++/include/list
    gblibstdc++/include/initializer_list
    gblibstdc++/include/new
    gblibstdc++/include/queue
    gblibstdc++/include/string
    gblibstdc++/include/set
    gblibstdc++/include/vector
    gblibstdc++/include/functional
    gblibstdc++/include/utility
    )

target_include_directories(gblibstdc++ PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
set_target_properties(gblibstdc++ PROPERTIES PUBLIC_HEADER "${GBSTDLIBCPP_PUBLIC_HEADERS}")
