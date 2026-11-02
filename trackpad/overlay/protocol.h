#pragma once
#include <windows.h>
#include <cstdint>
#include <cstddef>
#include <cmath>
namespace protocol {
constexpr wchar_t Mapping[]=L"Local\\TrackpadGestures.Overlay";
constexpr wchar_t Wake[]=L"Local\\TrackpadGestures.OverlayWake";
constexpr wchar_t Running[]=L"Local\\TrackpadOverlay.Running";
constexpr wchar_t WindowClass[]=L"TrackpadOverlay.Compositor";
constexpr uint32_t Magic=0x32505754;
struct State { uint32_t magic,seq,gesture,phase; float progress,velocity; uint64_t qpc; uint32_t session; };
static_assert(offsetof(State,qpc)==24 && offsetof(State,session)==32);
inline uint64_t now(){LARGE_INTEGER v;QueryPerformanceCounter(&v);return v.QuadPart;}
inline double frequency(){LARGE_INTEGER v;QueryPerformanceFrequency(&v);return double(v.QuadPart);}
inline bool read(const State* memory, State& out){
 // Aligned 32-bit loads are atomic on the supported x64 target. Do not use
 // InterlockedCompareExchange here: the overlay maps this page read-only.
 for(int i=0;i<32;i++) {auto a=*(volatile const uint32_t*)&memory->seq;if(a&1)continue;
 MemoryBarrier(); memcpy(&out,memory,sizeof(out));MemoryBarrier();
 auto b=*(volatile const uint32_t*)&memory->seq;
 if(a==b && out.magic==Magic && out.gesture<=3 && out.phase<=4 && std::isfinite(out.progress) && std::isfinite(out.velocity))return true;
 }return false;
}
}
