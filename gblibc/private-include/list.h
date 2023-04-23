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

void __node_insert(list_node* node, list_node* new_node);
void __node_erase(list_node* node);

#define NDINSERT(node, newnode) __node_insert(node, newnode)
#define NDERASE(p_node) __node_erase(p_node)

#ifdef __cplusplus
}
#endif

#endif
