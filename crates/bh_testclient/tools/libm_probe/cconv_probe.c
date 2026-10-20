#include <stdio.h>
#include <stdint.h>
#include <math.h>
#include <string.h>
int main(){
  volatile float fs[] = {NAN, INFINITY, -INFINITY, 2147483520.0f, 2147483648.0f, -2147483648.0f, -2147483904.0f, 3e9f, 0.999999f, -0.999999f, -0.0f, 1e-45f, 8388607.5f, -1.0f, 4.0e9f, 1e20f, 40000.0f, 300.5f, 255.9f, 200.0f, 9.223372e18f, 1.8446744e19f, 9223371487098961920.0f, 1.5e19f, -3e9f, 4294967296.0f};
  volatile double ds[] = {2147483647.9, 2147483648.0, -2147483648.9, -2147483649.0, 4294967295.0, 4294967296.0, 1e19, -3e9, 1.5e19, -1.0, 300.5, 40000.0, 1.8446744073709552e19, 9223372036854775808.0, 0.0/0.0};
  int n=sizeof fs/sizeof fs[0];
  for(int i=0;i<n;i++){ float f=fs[i]; uint32_t b; memcpy(&b,&f,4);
    printf("F %08x i32=%d i64=%ld u32=%u u64=%lu i16=%d u16=%u i8=%d u8=%u\n", b,(int)f,(long)f,(unsigned)f,(unsigned long)f,(short)(int)f,(unsigned short)(int)f,(signed char)(int)f,(unsigned char)(int)f);}
  int m=sizeof ds/sizeof ds[0];
  for(int i=0;i<m;i++){ double f=ds[i]; uint64_t b; memcpy(&b,&f,8);
    printf("D %016lx i32=%d i64=%ld u32=%u u64=%lu i16=%d u16=%u i8=%d u8=%u\n", b,(int)f,(long)f,(unsigned)f,(unsigned long)f,(short)(int)f,(unsigned short)(int)f,(signed char)(int)f,(unsigned char)(int)f);}
  return 0;}
