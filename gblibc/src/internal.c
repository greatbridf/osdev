#include <list.h>
#include <stdlib.h>

void __node_insert(list_node* node, list_node* newnode)
{
    NDNEXT(*newnode) = NDNEXT(*node);
    if (NDNEXT(*node))
        NDPREV(*NDNEXT(*node)) = newnode;
    NDNEXT(*node) = newnode;
    NDPREV(*newnode) = node;
}

void __node_erase(list_node* node)
{
    if (NDPREV(*node))
        NDNEXT(*NDPREV(*node)) = NDNEXT(*node);
    if (NDNEXT(*node))
        NDPREV(*NDNEXT(*node)) = NDPREV(*node);
    free(node);
}
