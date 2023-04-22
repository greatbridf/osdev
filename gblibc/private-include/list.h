#ifndef __GBLIBC_LIST_H_
#define __GBLIBC_LIST_H_

#ifdef __cplusplus
extern "C" {
#endif

struct list_node {
    struct list_node* prev;
    struct list_node* next;
    char data[];
};

typedef struct list_node list_node;
typedef list_node list_head;

#define NDDATA(node, type) (*((type*)((node).data)))
#define NDPREV(node) ((node).prev)
#define NDNEXT(node) ((node).next)
#define NDISEND(node) (!((node).next))
#define NEWNODE(type) ((struct list_node*)malloc(sizeof(list_node) + sizeof(type)))
#define NDPTR(p_data) ((list_node*)((char*)p_data - sizeof(list_node)))

#define NDINSERT(node, newnode)          \
    NDNEXT(*newnode) = NDNEXT(node);     \
    if (NDNEXT(node))                    \
        NDPREV(*NDNEXT(node)) = newnode; \
    NDNEXT(node) = newnode;              \
    NDPREV(*newnode) = &node

#define NDERASE(p_node)                             \
    if (NDPREV(*p_node))                            \
        NDNEXT(*NDPREV(*p_node)) = NDNEXT(*p_node); \
    if (NDNEXT(*p_node))                            \
        NDPREV(*NDNEXT(*p_node)) = NDPREV(*p_node); \
    free(p_node)

#ifdef __cplusplus
}
#endif

#endif
