#pragma once

namespace types::list {

template <typename ListNode>
void list_insert(ListNode** head, ListNode* node) {
    node->prev = nullptr;
    node->next = *head;
    if (*head)
        (*head)->prev = node;
    *head = node;
}

template <typename ListNode>
ListNode* list_get(ListNode** head) {
    ListNode* node = *head;
    if (node) {
        *head = node->next;

        node->next = nullptr;
        node->prev = nullptr;
    }
    return node;
}

template <typename ListNode>
void list_remove(ListNode** head, ListNode* node) {
    if (node->prev)
        node->prev->next = node->next;
    else
        *head = node->next;

    if (node->next)
        node->next->prev = node->prev;

    node->next = nullptr;
    node->prev = nullptr;
}

} // namespace types::list
