/* golden generator for fmath::vector (gcc -O0 -ffp-contract=off in the reference image) */
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
static uint64_t st = 88172645463325252ULL;
static uint64_t xs(void){ st ^= st<<13; st ^= st>>7; st ^= st<<17; return st; }
static float rf(float lo, float hi){ return lo + (hi-lo) * (float)((double)(xs()>>11) / 9007199254740992.0); }
static uint32_t b(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
typedef struct { float x, y; } V2;
static V2 sub(V2 a, V2 c){ V2 r = {a.x-c.x, a.y-c.y}; return r; }
static V2 add(V2 a, V2 c){ V2 r = {a.x+c.x, a.y+c.y}; return r; }
static V2 mulf(V2 a, float f){ V2 r = {a.x*f, a.y*f}; return r; }
static V2 spline(V2 aa, V2 a, V2 bq, V2 bb, float t, float factor){
  float t2 = t*t, t3 = t2*t;
  float h1 = ((t3+t3) - t2*3.0f) + 1.0f;
  float h2 = t3*-2.0f + t2*3.0f;
  float h3 = (t3 - (t2+t2)) + t;
  float h4 = t3 - t2;
  V2 tan1 = mulf(sub(bq,aa), factor), tan2 = mulf(sub(bb,a), factor);
  return add(add(add(mulf(a,h1), mulf(bq,h2)), mulf(tan1,h3)), mulf(tan2,h4));
}
static float ss(float x){ return x*x*(3.0f-2.0f*x); }
static float smooth(float a, float bq, float x){ if (x < a) return 0.0f; if (x < bq) { float t=(x-a)/(bq-a); return t*t*(3.0f-2.0f*t);} return 1.0f; }
int main(void){
  for (int i=0;i<3000;i++){ float a=rf(-100,100), c=rf(-100,100), f=rf(-2,2); float r = a + (c-a)*f; printf("L %08x %08x %08x %08x\n", b(a),b(c),b(f),b(r)); }
  for (int i=0;i<1500;i++){ V2 p[4]; for(int k=0;k<4;k++){p[k].x=rf(-50,50);p[k].y=rf(-50,50);} float t=rf(0,1), fa=rf(0,1.5f);
    V2 r = spline(p[0],p[1],p[2],p[3],t,fa);
    printf("S"); for(int k=0;k<4;k++) printf(" %08x %08x", b(p[k].x), b(p[k].y)); printf(" %08x %08x %08x %08x\n", b(t), b(fa), b(r.x), b(r.y)); }
  for (int i=0;i<1500;i++){ float x=rf(-1,2); printf("M %08x %08x\n", b(x), b(ss(x))); }
  for (int i=0;i<1500;i++){ float a=rf(-1,1), c=rf(1,3), x=rf(-2,4); printf("T %08x %08x %08x %08x\n", b(a),b(c),b(x),b(smooth(a,c,x))); }
  for (int i=0;i<1500;i++){ float x=rf(-10,10), y=rf(-10,10), z=rf(-10,10);
    float ls = (x*x + y*y) + z*z; float len = (float)sqrt((double)ls);
    float nx,ny,nz; if (len == 0.0f) {nx=ny=nz=0.0f;} else {nx=x/len;ny=y/len;nz=z/len;}
    float l2s = x*x + y*y; float l2 = l2s > 0.0f ? sqrtf(l2s) : 0.0f;
    printf("N %08x %08x %08x %08x %08x %08x %08x %08x\n", b(x),b(y),b(z),b(len),b(nx),b(ny),b(nz),b(l2)); }
  return 0; }
