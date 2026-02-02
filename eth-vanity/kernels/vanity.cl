// =============================================================================
// Ethereum Vanity Address Generator - OpenCL Kernel
// =============================================================================
//
// Incremental key approach:
//   CPU generates base public key Q = k * G
//   GPU computes Q + i*G for each work item i
//   Then: keccak256(pubkey) -> address -> pattern match
//
// Uses secp256k1 curve: y^2 = x^3 + 7 over F_p
//   p = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
// =============================================================================

// ---------------------------------------------------------------------------
// 256-bit unsigned integer type (8 x 32-bit limbs, little-endian)
// ---------------------------------------------------------------------------
typedef struct {
    uint d[8];
} uint256_t;

// secp256k1 field prime p
__constant uint256_t SECP256K1_P = {{
    0xFFFFFC2Fu, 0xFFFFFFFEu, 0xFFFFFFFFu, 0xFFFFFFFFu,
    0xFFFFFFFFu, 0xFFFFFFFFu, 0xFFFFFFFFu, 0xFFFFFFFFu
}};

// ---------------------------------------------------------------------------
// 256-bit arithmetic
// ---------------------------------------------------------------------------

// a == 0?
static bool uint256_is_zero(const uint256_t *a) {
    return (a->d[0] | a->d[1] | a->d[2] | a->d[3] |
            a->d[4] | a->d[5] | a->d[6] | a->d[7]) == 0;
}

// a == b?
static bool uint256_eq(const uint256_t *a, const uint256_t *b) {
    return (a->d[0] == b->d[0]) && (a->d[1] == b->d[1]) &&
           (a->d[2] == b->d[2]) && (a->d[3] == b->d[3]) &&
           (a->d[4] == b->d[4]) && (a->d[5] == b->d[5]) &&
           (a->d[6] == b->d[6]) && (a->d[7] == b->d[7]);
}

// a >= b?
static bool uint256_gte(const uint256_t *a, const uint256_t *b) {
    for (int i = 7; i >= 0; i--) {
        if (a->d[i] > b->d[i]) return true;
        if (a->d[i] < b->d[i]) return false;
    }
    return true; // equal
}

// r = a + b, returns carry
static uint uint256_add(uint256_t *r, const uint256_t *a, const uint256_t *b) {
    uint carry = 0;
    for (int i = 0; i < 8; i++) {
        ulong sum = (ulong)a->d[i] + (ulong)b->d[i] + (ulong)carry;
        r->d[i] = (uint)sum;
        carry = (uint)(sum >> 32);
    }
    return carry;
}

// r = a - b, returns borrow
static uint uint256_sub(uint256_t *r, const uint256_t *a, const uint256_t *b) {
    uint borrow = 0;
    for (int i = 0; i < 8; i++) {
        ulong diff = (ulong)a->d[i] - (ulong)b->d[i] - (ulong)borrow;
        r->d[i] = (uint)diff;
        borrow = (diff >> 63) & 1u;
    }
    return borrow;
}

// ---------------------------------------------------------------------------
// Modular arithmetic over F_p (secp256k1 field)
// ---------------------------------------------------------------------------

// r = (a + b) mod p
static void fp_add(uint256_t *r, const uint256_t *a, const uint256_t *b) {
    uint carry = uint256_add(r, a, b);
    uint256_t p = SECP256K1_P;
    if (carry || uint256_gte(r, &p)) {
        uint256_sub(r, r, &p);
    }
}

// r = (a - b) mod p
static void fp_sub(uint256_t *r, const uint256_t *a, const uint256_t *b) {
    uint borrow = uint256_sub(r, a, b);
    if (borrow) {
        uint256_t p = SECP256K1_P;
        uint256_add(r, r, &p);
    }
}

// r = (a * b) mod p using schoolbook multiplication + Barrett-like reduction
// We use a 512-bit intermediate product
static void fp_mul(uint256_t *r, const uint256_t *a, const uint256_t *b) {
    ulong prod[16];
    for (int i = 0; i < 16; i++) prod[i] = 0;

    // Schoolbook multiply: 8x8 -> 16 limbs
    for (int i = 0; i < 8; i++) {
        ulong carry = 0;
        for (int j = 0; j < 8; j++) {
            ulong t = prod[i + j] + (ulong)a->d[i] * (ulong)b->d[j] + carry;
            prod[i + j] = t & 0xFFFFFFFFUL;
            carry = t >> 32;
        }
        prod[i + 8] += carry;
    }

    // Reduction mod p using the special form of secp256k1 prime:
    // p = 2^256 - 0x1000003D1
    // So for a 512-bit number T = T_hi * 2^256 + T_lo:
    //   T mod p = T_lo + T_hi * 0x1000003D1 (mod p)
    // We may need to repeat since the multiply can overflow slightly.
    //
    // c = 0x1000003D1 (fits in 37 bits)
    // We multiply the high 256 bits by c and add to low 256 bits.

    ulong c_lo = 0x1000003D1UL;

    // First reduction: multiply high 8 limbs by c and add to low
    ulong carry = 0;
    for (int i = 0; i < 8; i++) {
        ulong t = prod[i] + prod[i + 8] * c_lo + carry;
        prod[i] = t & 0xFFFFFFFFUL;
        carry = t >> 32;
    }
    // carry can be up to ~37 bits worth; treat as a small "high" value
    // Multiply carry by c_lo and add back
    ulong extra = carry * c_lo;
    carry = 0;
    for (int i = 0; i < 8; i++) {
        ulong t = prod[i] + (extra & 0xFFFFFFFFUL) + carry;
        prod[i] = t & 0xFFFFFFFFUL;
        carry = t >> 32;
        extra >>= 32;
        if (i >= 1 && extra == 0 && carry == 0) {
            // Copy remaining limbs
            for (int j = i + 1; j < 8; j++) {
                r->d[j] = (uint)prod[j];
            }
            for (int j = 0; j <= i; j++) {
                r->d[j] = (uint)prod[j];
            }
            {
                uint256_t p = SECP256K1_P;
                if (uint256_gte(r, &p)) {
                    uint256_sub(r, r, &p);
                }
            }
            return;
        }
    }

    for (int i = 0; i < 8; i++) {
        r->d[i] = (uint)prod[i];
    }

    // Final conditional subtraction
    {
        uint256_t p = SECP256K1_P;
        if (uint256_gte(r, &p)) {
            uint256_sub(r, r, &p);
        }
    }
}

// r = a^2 mod p (uses fp_mul)
static void fp_sqr(uint256_t *r, const uint256_t *a) {
    fp_mul(r, a, a);
}

// r = a^(-1) mod p using Fermat's little theorem: a^(p-2) mod p
// Optimized addition chain for secp256k1
static void fp_inv(uint256_t *r, const uint256_t *a) {
    // p - 2 = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2D
    // Use a square-and-multiply chain.
    // We use the standard approach with some precomputation.

    uint256_t x2, x3, x6, x9, x11, x22, x44, x88, x176, x220, x223, t;

    // x2 = a^(2^2-1)
    fp_sqr(&x2, a);
    fp_mul(&x2, &x2, a);

    // x3 = a^(2^3-1)
    fp_sqr(&x3, &x2);
    fp_mul(&x3, &x3, a);

    // x6 = a^(2^6-1)
    t = x3;
    for (int i = 0; i < 3; i++) fp_sqr(&t, &t);
    fp_mul(&x6, &t, &x3);

    // x9 = a^(2^9-1)
    t = x6;
    for (int i = 0; i < 3; i++) fp_sqr(&t, &t);
    fp_mul(&x9, &t, &x3);

    // x11 = a^(2^11-1)
    t = x9;
    for (int i = 0; i < 2; i++) fp_sqr(&t, &t);
    fp_mul(&x11, &t, &x2);

    // x22 = a^(2^22-1)
    t = x11;
    for (int i = 0; i < 11; i++) fp_sqr(&t, &t);
    fp_mul(&x22, &t, &x11);

    // x44 = a^(2^44-1)
    t = x22;
    for (int i = 0; i < 22; i++) fp_sqr(&t, &t);
    fp_mul(&x44, &t, &x22);

    // x88 = a^(2^88-1)
    t = x44;
    for (int i = 0; i < 44; i++) fp_sqr(&t, &t);
    fp_mul(&x88, &t, &x44);

    // x176 = a^(2^176-1)
    t = x88;
    for (int i = 0; i < 88; i++) fp_sqr(&t, &t);
    fp_mul(&x176, &t, &x88);

    // x220 = a^(2^220-1)
    t = x176;
    for (int i = 0; i < 44; i++) fp_sqr(&t, &t);
    fp_mul(&x220, &t, &x44);

    // x223 = a^(2^223-1)
    t = x220;
    for (int i = 0; i < 3; i++) fp_sqr(&t, &t);
    fp_mul(&x223, &t, &x3);

    // Final: a^(p-2)
    // p-2 = (2^223 - 1) * 2^33 + 2^32 - 0x3D3
    // = x223 * 2^33 + ...
    // More precisely:
    // p-2 in binary ends with: ...1111 1111 1111 1110 1111 1111 1100 0010 1101
    // The last 32 bits of p-2 are 0xFFFFFC2D

    t = x223;
    for (int i = 0; i < 23; i++) fp_sqr(&t, &t);
    fp_mul(&t, &t, &x22);
    for (int i = 0; i < 5; i++) fp_sqr(&t, &t);
    fp_mul(&t, &t, a);
    for (int i = 0; i < 3; i++) fp_sqr(&t, &t);
    fp_mul(&t, &t, &x2);
    fp_sqr(&t, &t);
    fp_sqr(&t, &t);

    *r = t;
}

// ---------------------------------------------------------------------------
// Elliptic curve point (affine coordinates)
// ---------------------------------------------------------------------------
typedef struct {
    uint256_t x;
    uint256_t y;
    bool infinity;
} ec_point_t;

// Point doubling: R = 2*P (affine)
static void ec_double(ec_point_t *r, const ec_point_t *p) {
    if (p->infinity) {
        *r = *p;
        return;
    }

    uint256_t zero = {{0,0,0,0,0,0,0,0}};
    if (uint256_eq(&p->y, &zero)) {
        r->infinity = true;
        return;
    }

    // lambda = (3 * x^2) / (2 * y)
    uint256_t x_sq, num, denom, two_y, lambda, lambda_sq;

    fp_sqr(&x_sq, &p->x);
    fp_add(&num, &x_sq, &x_sq);
    fp_add(&num, &num, &x_sq); // num = 3 * x^2

    fp_add(&two_y, &p->y, &p->y); // denom = 2 * y
    fp_inv(&denom, &two_y);

    fp_mul(&lambda, &num, &denom);

    // x_r = lambda^2 - 2*x
    fp_sqr(&lambda_sq, &lambda);
    fp_sub(&r->x, &lambda_sq, &p->x);
    fp_sub(&r->x, &r->x, &p->x);

    // y_r = lambda * (x - x_r) - y
    uint256_t dx;
    fp_sub(&dx, &p->x, &r->x);
    fp_mul(&r->y, &lambda, &dx);
    fp_sub(&r->y, &r->y, &p->y);

    r->infinity = false;
}

// Point addition: R = P + Q (affine)
static void ec_add(ec_point_t *r, const ec_point_t *p, const ec_point_t *q) {
    if (p->infinity) { *r = *q; return; }
    if (q->infinity) { *r = *p; return; }

    if (uint256_eq(&p->x, &q->x)) {
        if (uint256_eq(&p->y, &q->y)) {
            ec_double(r, p);
            return;
        }
        // P + (-P) = O
        r->infinity = true;
        return;
    }

    // lambda = (y2 - y1) / (x2 - x1)
    uint256_t dy, dx, dx_inv, lambda, lambda_sq;

    fp_sub(&dy, &q->y, &p->y);
    fp_sub(&dx, &q->x, &p->x);
    fp_inv(&dx_inv, &dx);
    fp_mul(&lambda, &dy, &dx_inv);

    // x_r = lambda^2 - x1 - x2
    fp_sqr(&lambda_sq, &lambda);
    fp_sub(&r->x, &lambda_sq, &p->x);
    fp_sub(&r->x, &r->x, &q->x);

    // y_r = lambda * (x1 - x_r) - y1
    uint256_t t;
    fp_sub(&t, &p->x, &r->x);
    fp_mul(&r->y, &lambda, &t);
    fp_sub(&r->y, &r->y, &p->y);

    r->infinity = false;
}

// ---------------------------------------------------------------------------
// Precomputed table of i*G for i = 0..255 is passed as a buffer
// Format: 256 entries of (x, y) as 64 bytes each (big-endian)
//
// Scalar multiplication: compute offset * G using the table
//   offset is a 32-bit unsigned integer (the work item index)
//   We decompose offset into bytes and use a precomputed table.
//
// Actually, we use a more efficient approach:
//   Table has 32 entries: 2^0 * G, 2^1 * G, ..., 2^31 * G
//   We add the relevant entries based on set bits.
// ---------------------------------------------------------------------------

// Load a uint256 from big-endian byte buffer
static uint256_t load_uint256_be(__global const uchar *buf) {
    uint256_t r;
    for (int i = 0; i < 8; i++) {
        int off = (7 - i) * 4;
        r.d[i] = ((uint)buf[off] << 24) | ((uint)buf[off+1] << 16) |
                  ((uint)buf[off+2] << 8) | (uint)buf[off+3];
    }
    return r;
}

// Load a point from the precomputed G table
// Table layout: 32 entries, each 64 bytes (32 bytes x, 32 bytes y), big-endian
static ec_point_t load_g_table_entry(__global const uchar *table, int index) {
    ec_point_t pt;
    __global const uchar *entry = table + (ulong)index * 64;
    pt.x = load_uint256_be(entry);
    pt.y = load_uint256_be(entry + 32);
    pt.infinity = false;
    return pt;
}

// Compute offset * G using precomputed table of 2^k * G
static ec_point_t scalar_mul_g(uint offset, __global const uchar *g_table) {
    ec_point_t result;
    result.infinity = true;
    result.x = (uint256_t){{0,0,0,0,0,0,0,0}};
    result.y = (uint256_t){{0,0,0,0,0,0,0,0}};

    for (int bit = 0; bit < 32; bit++) {
        if (offset & (1u << bit)) {
            ec_point_t g_pow = load_g_table_entry(g_table, bit);
            ec_point_t tmp;
            ec_add(&tmp, &result, &g_pow);
            result = tmp;
        }
    }
    return result;
}

// ---------------------------------------------------------------------------
// Keccak-256 (NOT SHA3-256: uses padding byte 0x01, not 0x06)
// ---------------------------------------------------------------------------
// Keccak-f[1600] permutation over 25 x 64-bit state

// Rotation constants
__constant int keccak_rotc[24] = {
     1,  3,  6, 10, 15, 21, 28, 36,
    45, 55,  2, 14, 27, 41, 56,  8,
    25, 43, 62, 18, 39, 21, 56, 14
};

// Pi permutation indices
__constant int keccak_piln[24] = {
    10,  7, 11, 17, 18,  3,  5, 16,
     8, 21, 24,  4, 15, 23, 19, 13,
    12,  2, 20, 14, 22,  9,  6,  1
};

// Round constants
__constant ulong keccak_rndc[24] = {
    0x0000000000000001UL, 0x0000000000008082UL, 0x800000000000808aUL,
    0x8000000080008000UL, 0x000000000000808bUL, 0x0000000080000001UL,
    0x8000000080008081UL, 0x8000000000008009UL, 0x000000000000008aUL,
    0x0000000000000088UL, 0x0000000080008009UL, 0x000000008000000aUL,
    0x000000008000808bUL, 0x800000000000008bUL, 0x8000000000008089UL,
    0x8000000000008003UL, 0x8000000000008002UL, 0x8000000000000080UL,
    0x000000000000800aUL, 0x800000008000000aUL, 0x8000000080008081UL,
    0x8000000000008080UL, 0x0000000080000001UL, 0x8000000080008008UL
};

static ulong rotl64(ulong x, int n) {
    return (x << n) | (x >> (64 - n));
}

static void keccak_f1600(ulong st[25]) {
    for (int round = 0; round < 24; round++) {
        // Theta
        ulong bc[5];
        for (int i = 0; i < 5; i++)
            bc[i] = st[i] ^ st[i+5] ^ st[i+10] ^ st[i+15] ^ st[i+20];

        for (int i = 0; i < 5; i++) {
            ulong t = bc[(i+4)%5] ^ rotl64(bc[(i+1)%5], 1);
            for (int j = 0; j < 25; j += 5)
                st[j+i] ^= t;
        }

        // Rho and Pi
        ulong t = st[1];
        for (int i = 0; i < 24; i++) {
            int j = keccak_piln[i];
            ulong tmp = st[j];
            st[j] = rotl64(t, keccak_rotc[i]);
            t = tmp;
        }

        // Chi
        for (int j = 0; j < 25; j += 5) {
            ulong tmp[5];
            for (int i = 0; i < 5; i++) tmp[i] = st[j+i];
            for (int i = 0; i < 5; i++)
                st[j+i] = tmp[i] ^ ((~tmp[(i+1)%5]) & tmp[(i+2)%5]);
        }

        // Iota
        st[0] ^= keccak_rndc[round];
    }
}

// Keccak-256 hash of exactly 64 bytes (uncompressed pubkey without prefix)
// rate = 136 bytes for Keccak-256 (capacity = 512 bits)
static void keccak256_64bytes(const uchar input[64], uchar output[32]) {
    ulong st[25];
    for (int i = 0; i < 25; i++) st[i] = 0;

    // Absorb 64 bytes (8 lanes of 8 bytes each)
    for (int i = 0; i < 8; i++) {
        ulong lane = 0;
        for (int j = 0; j < 8; j++) {
            lane |= (ulong)input[i*8 + j] << (j * 8);
        }
        st[i] ^= lane;
    }

    // Padding: Keccak (not SHA3) uses pad10*1 with domain byte 0x01
    // byte 64 gets XOR 0x01 (domain separation for Keccak)
    // byte 135 (last byte of rate) gets XOR 0x80
    // byte 64 is in lane 8 (64/8 = 8), offset 0
    st[8] ^= 0x01UL;
    // byte 135 is in lane 16 (128/8 = 16), offset 7 (135 - 128 = 7)
    st[16] ^= 0x80UL << 56;

    // Permute
    keccak_f1600(st);

    // Squeeze: first 32 bytes = 4 lanes
    for (int i = 0; i < 4; i++) {
        for (int j = 0; j < 8; j++) {
            output[i*8 + j] = (uchar)(st[i] >> (j * 8));
        }
    }
}

// ---------------------------------------------------------------------------
// Store uncompressed public key (64 bytes, big-endian x || y)
// ---------------------------------------------------------------------------
static void point_to_pubkey(const ec_point_t *pt, uchar pubkey[64]) {
    // x in big-endian
    for (int i = 0; i < 8; i++) {
        int off = (7 - i) * 4;
        pubkey[off]   = (uchar)(pt->x.d[i] >> 24);
        pubkey[off+1] = (uchar)(pt->x.d[i] >> 16);
        pubkey[off+2] = (uchar)(pt->x.d[i] >> 8);
        pubkey[off+3] = (uchar)(pt->x.d[i]);
    }
    // y in big-endian
    for (int i = 0; i < 8; i++) {
        int off = 32 + (7 - i) * 4;
        pubkey[off]   = (uchar)(pt->y.d[i] >> 24);
        pubkey[off+1] = (uchar)(pt->y.d[i] >> 16);
        pubkey[off+2] = (uchar)(pt->y.d[i] >> 8);
        pubkey[off+3] = (uchar)(pt->y.d[i]);
    }
}

// ---------------------------------------------------------------------------
// Pattern matching on address nibbles
// ---------------------------------------------------------------------------
// Pattern config (passed from host):
//   pattern_type: 0=prefix, 1=suffix, 2=contains, 3=prefix+suffix
//   pattern_len: length of prefix pattern in nibbles
//   suffix_len: length of suffix pattern in nibbles
//   pattern_nibbles[40]: prefix pattern as nibbles (0-15)
//   suffix_nibbles[40]: suffix pattern as nibbles (0-15)
//   case_sensitive: not applicable on GPU (we always match lowercase hex)

typedef struct {
    uint pattern_type;   // 0=prefix, 1=suffix, 2=contains, 3=prefix+suffix
    uint pattern_len;    // prefix pattern length in nibbles
    uint suffix_len;     // suffix pattern length in nibbles
    uint _pad;
    uchar pattern_nibbles[40];
    uchar suffix_nibbles[40];
} gpu_pattern_config_t;

// Get nibble from address bytes (20 bytes = 40 nibbles)
static uchar get_nibble(const uchar addr[20], int idx) {
    uchar byte = addr[idx >> 1];
    return (idx & 1) ? (byte & 0x0f) : (byte >> 4);
}

static bool match_pattern_at(const uchar addr[20], const uchar *nibbles, uint len, int start) {
    for (uint i = 0; i < len; i++) {
        if (get_nibble(addr, start + (int)i) != nibbles[i])
            return false;
    }
    return true;
}

static bool pattern_matches(const uchar addr[20], __global const gpu_pattern_config_t *cfg) {
    // Load pattern config into private memory for faster access
    uint ptype = cfg->pattern_type;
    uint plen = cfg->pattern_len;
    uint slen = cfg->suffix_len;

    uchar pnib[40], snib[40];
    for (uint i = 0; i < plen; i++) pnib[i] = cfg->pattern_nibbles[i];
    for (uint i = 0; i < slen; i++) snib[i] = cfg->suffix_nibbles[i];

    if (ptype == 0) {
        // Prefix
        return match_pattern_at(addr, pnib, plen, 0);
    } else if (ptype == 1) {
        // Suffix
        return match_pattern_at(addr, snib, slen, 40 - (int)slen);
    } else if (ptype == 2) {
        // Contains
        int limit = 40 - (int)plen;
        for (int start = 0; start <= limit; start++) {
            if (match_pattern_at(addr, pnib, plen, start))
                return true;
        }
        return false;
    } else if (ptype == 3) {
        // Prefix + Suffix
        bool prefix_ok = match_pattern_at(addr, pnib, plen, 0);
        bool suffix_ok = match_pattern_at(addr, snib, slen, 40 - (int)slen);
        return prefix_ok && suffix_ok;
    }
    return false;
}

// ---------------------------------------------------------------------------
// Result buffer entry
// ---------------------------------------------------------------------------
typedef struct {
    uint found;    // 1 if match found, 0 otherwise
    uint offset;   // work item index that found it
    uchar addr[20]; // the matching address
} gpu_result_t;

// ---------------------------------------------------------------------------
// Main kernel
// ---------------------------------------------------------------------------
__kernel void vanity_iterate_and_match(
    __global const uchar *base_pubkey,        // 64 bytes: base public key (x || y) big-endian
    __global const uchar *g_table,            // 32 * 64 bytes: precomputed 2^k * G table
    __global const gpu_pattern_config_t *cfg,  // pattern configuration
    __global gpu_result_t *results,           // result buffer (max_results entries)
    __global volatile uint *result_count,     // atomic counter for results
    const uint max_results,                   // max result slots
    const uint batch_offset                   // added to global_id for offset
    ) {
    uint gid = get_global_id(0);
    uint offset = gid + batch_offset;

    // Skip offset 0 (that's just the base key itself, already checked by CPU if needed)
    if (offset == 0) return;

    // Load base public key
    ec_point_t base_pt;
    base_pt.x = load_uint256_be(base_pubkey);
    base_pt.y = load_uint256_be(base_pubkey + 32);
    base_pt.infinity = false;

    // Compute offset * G
    ec_point_t offset_g = scalar_mul_g(offset, g_table);

    // Compute Q = base_pt + offset * G
    ec_point_t q;
    ec_add(&q, &base_pt, &offset_g);

    if (q.infinity) return;

    // Serialize public key
    uchar pubkey[64];
    point_to_pubkey(&q, pubkey);

    // Keccak-256 hash
    uchar hash[32];
    keccak256_64bytes(pubkey, hash);

    // Address = last 20 bytes of hash
    uchar addr[20];
    for (int i = 0; i < 20; i++) {
        addr[i] = hash[i + 12];
    }

    // Pattern match
    if (pattern_matches(addr, cfg)) {
        uint idx = atomic_inc(result_count);
        if (idx < max_results) {
            results[idx].found = 1;
            results[idx].offset = offset;
            for (int i = 0; i < 20; i++) {
                results[idx].addr[i] = addr[i];
            }
        }
    }
}
