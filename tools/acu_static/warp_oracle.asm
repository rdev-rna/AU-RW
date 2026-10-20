; Isolated ACU translation arithmetic only. No game process or installed bytes
; are modified. See ACU-RE-0031; stack/register contract is excerpt-specific.
EXTERN warp_entry:QWORD
.const
epsilon DWORD 03c23d70ah
one DWORD 03f800000h
abs_mask DWORD 07fffffffh,07fffffffh,07fffffffh,07fffffffh
.code
run_warp PROC
    push rbp
    push rbx
    sub rsp,400h
    mov [rsp+2f8h],rdx
    movups [rsp+300h],xmm6
    movups [rsp+310h],xmm7
    movups [rsp+320h],xmm8
    movups [rsp+330h],xmm9
    movups [rsp+340h],xmm10
    movups [rsp+350h],xmm11
    movups [rsp+360h],xmm12
    movups [rsp+370h],xmm13
    movups [rsp+380h],xmm14
    movups [rsp+390h],xmm15
    lea rbp,[rsp+0f8h]
    movups xmm0,[rcx]
    movups [rsp+58h],xmm0
    movups xmm0,[rcx+10h]
    movups [rbp-40h],xmm0
    movups xmm0,[rcx+20h]
    movups [rbp-20h],xmm0
    movss xmm12,DWORD PTR [rcx+30h]
    movss xmm0,DWORD PTR [rcx+34h]
    movss DWORD PTR [rsp+1ch],xmm0
    mov eax,[rcx+38h]
    mov [rbp+1f0h],eax
    movss xmm13,epsilon
    movss xmm14,one
    movups xmm15,abs_mask
    call QWORD PTR warp_entry
    mov rdx,[rsp+2f8h]
    movups xmm0,[rsp+38h]
    movups [rdx],xmm0
    movzx eax,bl
    mov [rdx+0ch],eax
    movups xmm6,[rsp+300h]
    movups xmm7,[rsp+310h]
    movups xmm8,[rsp+320h]
    movups xmm9,[rsp+330h]
    movups xmm10,[rsp+340h]
    movups xmm11,[rsp+350h]
    movups xmm12,[rsp+360h]
    movups xmm13,[rsp+370h]
    movups xmm14,[rsp+380h]
    movups xmm15,[rsp+390h]
    add rsp,400h
    pop rbx
    pop rbp
    ret
run_warp ENDP
END

