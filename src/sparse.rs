// Logic to interact with Android Sparse Image
// # Android sparse img format
// # From https://android.googlesource.com/\
// # platform/system/core/+/master/libsparse/sparse_format.h
// 0		lelong	0xed26ff3a		Android sparse image
// >4		leshort	x			\b, version: %d
// >6		leshort	x			\b.%d
// >16		lelong	x			\b, Total of %d
// >12		lelong	x			\b %d-byte output blocks in
// >20		lelong	x			\b %d input chunks.

#[repr(packed)]
struct Version {
    major: u16,
    minor: u16
}

#[repr(packed)]
struct Header {
    magic: u32,
    version: Version,
    
    

}