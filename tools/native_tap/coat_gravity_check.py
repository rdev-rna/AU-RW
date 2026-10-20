"""Focused symbolic check of the force-mode-1 SSE block, ACU-RE-0028."""
import struct
from pathlib import Path
import simd_symbolic as S
import tap_events as E

root=Path(__file__).resolve().parents[2]/'local/native-probe'
raw=bytearray((root/'startup-25828-138507781.mapped.bin').read_bytes())
code=(root/'s2-maxlookat/coat-code-45510-24480.bin').read_bytes()
raw[0x45510:0x45510+len(code)]=code
for off,value in [(0x2e76400,[0x80000000]*4),(0x361ea60,[0x3f000000]*4),(0x2e78010,[0x40000000]*3+[0])]:
    struct.pack_into('<4I',raw,off,*value)
e=next(e for e in E.parse(str(root/'s2-maxlookat/coat-stages-24480.bin'))
       if not e['phase'] and e['hook']==0 and struct.unpack_from('<H',e['snaps'][2],28)[0]==78)
node=e['snaps'][2]
v=lambda o:list(struct.unpack_from('<4f',node,o))
m=S.Emulator(raw,{'rcx':('node',0)})
m.store(('rbp',-16),v(256));m.store(('node',272),v(272))
m.store(('rbp',16),[S.Bits(0x80000000)]*4)
m.store(('rbp',0x1f0),[0.,0.,-9.81,0.])
m.store(('rbp',-0x70),v(240))
m.set('xmm3',[S.Bits(0x80000000)]*4)
m.run(0x1400461a8,0x140046214)
print('native product',m.get('xmm6'))
m.run(0x140046214,0x14004628e)
print('native gravity',m.get('xmm5'))
a,b=v(256),v(272)
q=[a[3]*b[0]+a[0]*b[3]+a[1]*b[2]-a[2]*b[1],
   a[3]*b[1]-a[0]*b[2]+a[1]*b[3]+a[2]*b[0],
   a[3]*b[2]+a[0]*b[1]-a[1]*b[0]+a[2]*b[3],
   a[3]*b[3]-a[0]*b[0]-a[1]*b[1]-a[2]*b[2]]
print('expected product',q)
dot=q[2]*-9.81
print('expected gravity',[2*(q[1]*-9.81*q[3]+q[0]*dot),
                         2*(q[0]*9.81*q[3]+q[1]*dot),
                         2*(q[2]*dot-9.81*(q[3]*q[3]-.5))])
import numpy as np
f=np.float32
v3=lambda off:np.array(v(off)[:3],dtype=np.float32)
dt=f(struct.unpack('<f',e['snaps'][10])[0]); olddt=f(struct.unpack_from('<f',e['snaps'][1],48)[0])
gravity=np.array([float(x) for x in m.get('xmm5')[:3]],dtype=np.float32)
velocity=(v3(224)-v3(240))/olddt
pred=v3(224)+gravity*(dt*dt)+velocity*dt
unit=lambda x:x/np.sqrt(np.sum(x*x))
correction=((unit(v3(240)-v3(208))-unit(pred-v3(192)))/olddt)*f(2)*f(.25)
pred+=correction*(dt*dt)
print('numeric',pred,'velocity',velocity,'correction',correction,'gravity',gravity)
es=E.parse(str(root/'s2-maxlookat/coat-stages-24480.bin'))
mid=next(x for x in es if x['hook']==1 and x['seq']>e['seq'])
print('captured',struct.unpack('<8f',mid['snaps'][20]))
