; Generic x64 hook thunk. Per-hook stubs set r10-free state: r11 = hook id, then jmp here.
; Preserves rcx,rdx,r8,r9,xmm0-3 and replays 4 shadow + 12 stack qwords to the original.
EXTRN TapOnEnter:PROC
EXTRN TapOnExit:PROC
EXTRN TapTrampolines:QWORD
.code
PUBLIC TapThunk
TapThunk PROC
    push rbp
    mov rbp, rsp
    sub rsp, 1C0h
    mov [rbp-100h], rcx
    mov [rbp-0F8h], rdx
    mov [rbp-0F0h], r8
    mov [rbp-0E8h], r9
    movups xmmword ptr [rbp-0E0h], xmm0
    movups xmmword ptr [rbp-0D0h], xmm1
    movups xmmword ptr [rbp-0C0h], xmm2
    movups xmmword ptr [rbp-0B0h], xmm3
    lea rax, [rbp+8]
    mov [rbp-0A0h], rax
    mov [rbp-98h], r11
    mov qword ptr [rbp-90h], 0
    mov qword ptr [rbp-88h], 0
    mov rcx, r11
    lea rdx, [rbp-100h]
    call TapOnEnter
    mov rcx, [rbp-100h]
    mov rdx, [rbp-0F8h]
    mov r8,  [rbp-0F0h]
    mov r9,  [rbp-0E8h]
    movups xmm0, xmmword ptr [rbp-0E0h]
    movups xmm1, xmmword ptr [rbp-0D0h]
    movups xmm2, xmmword ptr [rbp-0C0h]
    movups xmm3, xmmword ptr [rbp-0B0h]
    ; replay caller's shadow + stack args (16 qwords at [rbp+16]) to our outgoing area
    mov rax, [rbp+10h]
    mov [rsp], rax
    mov rax, [rbp+18h]
    mov [rsp+8], rax
    mov rax, [rbp+20h]
    mov [rsp+10h], rax
    mov rax, [rbp+28h]
    mov [rsp+18h], rax
    mov rax, [rbp+30h]
    mov [rsp+20h], rax
    mov rax, [rbp+38h]
    mov [rsp+28h], rax
    mov rax, [rbp+40h]
    mov [rsp+30h], rax
    mov rax, [rbp+48h]
    mov [rsp+38h], rax
    mov rax, [rbp+50h]
    mov [rsp+40h], rax
    mov rax, [rbp+58h]
    mov [rsp+48h], rax
    mov rax, [rbp+60h]
    mov [rsp+50h], rax
    mov rax, [rbp+68h]
    mov [rsp+58h], rax
    mov rax, [rbp+70h]
    mov [rsp+60h], rax
    mov rax, [rbp+78h]
    mov [rsp+68h], rax
    mov rax, [rbp+80h]
    mov [rsp+70h], rax
    mov rax, [rbp+88h]
    mov [rsp+78h], rax
    cmp qword ptr [rbp-88h], 0
    jne skip_original
    mov rax, [rbp-98h]
    lea r10, TapTrampolines
    mov r10, [r10+rax*8]
    call r10
    mov [rbp-90h], rax
    movups xmmword ptr [rbp-80h], xmm0
skip_original:
    mov rcx, [rbp-98h]
    lea rdx, [rbp-100h]
    call TapOnExit
    mov rax, [rbp-90h]
    movups xmm0, xmmword ptr [rbp-80h]
    leave
    ret
TapThunk ENDP
END
