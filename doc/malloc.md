[参考链接1](https://blog.codinglabs.org/articles/a-malloc-tutorial.html#212-%E9%A1%B5%E4%B8%8E%E5%9C%B0%E5%9D%80%E6%9E%84%E6%88%90)
[malloc的glibc实现](https://repo.or.cz/glibc.git/blob/HEAD:/malloc/malloc.c)
[How the kernel manage your memory](https://manybutfinite.com/post/how-the-kernel-manages-your-memory/)

## 地址空间分区
从0x00000000到0x3fffffff为内核空间

从0x40000000到0xffffffff为用户空间

### 内核空间：
0x00000000到0x2fffffff为动态映射区
0x30000000到0x3fffffff为永久映射区，这个区域的内存在与物理页进行映射后不会被交换出地址空间

## 物理内存分配
0x00000000-0x00000fff：内核页目录
0x00001000-0x00001fff：空白页

## 大致思路：
每个进程（包括内核）拥有一个struct mm，用于记录自身的虚拟地址映射状况

struct mm拥有struct page的链表，对应虚拟地址实际映射的物理页

struct mm的项目还可包括copy on write的页

发生缺页中断时，内核通过该触发中断的进程的struct mm检查页权限、是否需要复制页等

若因权限不足而触发中断，则内核中断用户进程执行，或内核panic

若因页已被交换出内存，则内核将页换回内存，继续进程执行

若因页为copy on write页，写入时触发中断，则将页复制一份，继续进程执行

### 内核提供几个接口
1. alloc_page从页位图中找出未被使用的物理页返回
2. p_map用于将物理页映射到指定的虚拟地址
3. kmap用于给出物理页，将其映射到一个未被该进程使用的虚拟地址

### 分配内存
通过kmap将空白页映射到某虚拟地址上，并开启copy on write，
随后则可以直接对该页进行读写，写入时内核中断自动进行页的分配

因此，换页中断的处理非常重要

## 注
分页相关的内存数据结构应始终被映射到永久映射区，并且该映射应在页表创建时被完成
