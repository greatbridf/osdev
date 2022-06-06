#pragma once

#define LIST_LIKE_AT(type, list_like, pos, result_name) \
    type* result_name = list_like; \
    {                   \
        size_t _tmp_pos = (pos); \
        while (_tmp_pos--) \
            result_name = result_name->next; \
    }
