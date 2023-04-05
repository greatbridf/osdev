#include <ctype.h>

int islower(int c)
{
    return c >= 'a' && c <= 'z';
}

int isupper(int c)
{
    return c >= 'A' && c <= 'Z';
}

int tolower(int c)
{
    if (isupper(c))
        return c - 'A' + 'a';
    return c;
}

int toupper(int c)
{
    if (islower(c))
        return c - 'a' + 'A';
    return c;
}
