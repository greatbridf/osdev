int* __errno_location(void)
{
    static int __errno = 0;
    return &__errno;
}
