MEMORY {
    LOWMEM : org = 0x0000000000000000, len = 1M
    VDSO   : org = 0x00007f0000000000, len = 4K
    KBSS   : org = 0xffffffffc0200000, len = 2M
    KIMAGE : org = 0xffffffffffc00000, len = 2M
}

REGION_ALIAS("REGION_TEXT", KIMAGE);
REGION_ALIAS("REGION_RODATA", KIMAGE);
REGION_ALIAS("REGION_DATA", KIMAGE);
REGION_ALIAS("REGION_BSS", KBSS);
REGION_ALIAS("REGION_EHFRAME", KIMAGE);

REGION_ALIAS("LINK_REGION_TEXT", KIMAGE);
REGION_ALIAS("LINK_REGION_RODATA", KIMAGE);
REGION_ALIAS("LINK_REGION_DATA", KIMAGE);
REGION_ALIAS("LINK_REGION_BSS", KBSS);
REGION_ALIAS("LINK_REGION_EHFRAME", KIMAGE);
