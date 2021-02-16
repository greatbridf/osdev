# Interrupt handling

When an interrupt is raised, the x86 cpu will
push some of the current working states into
current user stack. (At least for GE ints) in
the following order:

push  (EFLAGS)
push  %cs
pushl %eip
push  (ERROR_CODE)

The eip should points to the instruction that
caused the exception.

ERROR_CODE can be segment selector index (if
the exception is segment related) or simply
zero

Notice that the error code doesn't always 
exists. In some cases, it can be absent.
