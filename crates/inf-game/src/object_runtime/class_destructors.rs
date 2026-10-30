//! Each final table's +00 is the compiler's scalar deleting destructor: it
//! runs the class's destructor body, then frees the object when flag bit 0 is
//! set. The bodies end in the shared actor (0047AA60) or visual (00479D00)
//! destructor, some through a family's own; the disposal drain's
//! [`ObjectArena::destroy_with_process`](super::ObjectArena::destroy_with_process)
//! reproduces each chain's effects for the classes the port builds.

pub(crate) struct ClassDestructor {
    pub selectors: &'static [u16],
    pub deleting: u32,
    pub body: u32,
}

pub(crate) const CLASS_DESTRUCTORS: [ClassDestructor; 59] = [
    ClassDestructor {
        selectors: &[1],
        deleting: 0x0047_9ce0,
        body: 0x0047_9d00,
    },
    ClassDestructor {
        selectors: &[2],
        deleting: 0x0041_5af0,
        body: 0x0041_5b10,
    },
    ClassDestructor {
        selectors: &[3, 4, 5, 6],
        deleting: 0x0047_aa40,
        body: 0x0047_aa60,
    },
    ClassDestructor {
        selectors: &[7, 33],
        deleting: 0x0042_5f50,
        body: 0x0042_5f70,
    },
    ClassDestructor {
        selectors: &[8, 9],
        deleting: 0x0042_37b0,
        body: 0x0042_37d0,
    },
    ClassDestructor {
        selectors: &[10],
        deleting: 0x0042_51b0,
        body: 0x0042_51d0,
    },
    ClassDestructor {
        selectors: &[11],
        deleting: 0x0040_1570,
        body: 0x0040_1590,
    },
    ClassDestructor {
        selectors: &[12],
        deleting: 0x0046_71c0,
        body: 0x0046_71e0,
    },
    ClassDestructor {
        selectors: &[13],
        deleting: 0x0046_1b80,
        body: 0x0046_1bc0,
    },
    ClassDestructor {
        selectors: &[14, 89],
        deleting: 0x0046_e080,
        body: 0x0046_e0a0,
    },
    ClassDestructor {
        selectors: &[15],
        deleting: 0x0042_e300,
        body: 0x0042_e320,
    },
    ClassDestructor {
        selectors: &[16],
        deleting: 0x0040_f680,
        body: 0x0040_f6a0,
    },
    ClassDestructor {
        selectors: &[17],
        deleting: 0x0046_e6f0,
        body: 0x0046_e710,
    },
    ClassDestructor {
        selectors: &[18],
        deleting: 0x0047_68b0,
        body: 0x0047_68d0,
    },
    ClassDestructor {
        selectors: &[19, 22, 54, 58, 63, 64, 65, 71, 75, 76, 86, 92, 94],
        deleting: 0x0047_5400,
        body: 0x0040_4c20,
    },
    ClassDestructor {
        selectors: &[20],
        deleting: 0x0045_0420,
        body: 0x0045_0440,
    },
    ClassDestructor {
        selectors: &[21],
        deleting: 0x0040_55d0,
        body: 0x0040_55f0,
    },
    ClassDestructor {
        selectors: &[23],
        deleting: 0x0040_d850,
        body: 0x0040_d870,
    },
    ClassDestructor {
        selectors: &[24, 26, 29, 31, 32, 40, 41, 52, 53, 57, 69, 74],
        deleting: 0x0041_10e0,
        body: 0x0048_54b0,
    },
    ClassDestructor {
        selectors: &[25],
        deleting: 0x0045_82b0,
        body: 0x0045_82d0,
    },
    ClassDestructor {
        selectors: &[27],
        deleting: 0x0045_8fa0,
        body: 0x0045_62f0,
    },
    ClassDestructor {
        selectors: &[28],
        deleting: 0x0045_8fe0,
        body: 0x0045_6a70,
    },
    ClassDestructor {
        selectors: &[30, 37, 39],
        deleting: 0x0048_b200,
        body: 0x0048_b220,
    },
    ClassDestructor {
        selectors: &[34],
        deleting: 0x0043_3b70,
        body: 0x0043_3b90,
    },
    ClassDestructor {
        selectors: &[35],
        deleting: 0x0043_d0d0,
        body: 0x0043_d0f0,
    },
    ClassDestructor {
        selectors: &[36],
        deleting: 0x0048_1150,
        body: 0x0048_1170,
    },
    ClassDestructor {
        selectors: &[38],
        deleting: 0x0045_8fc0,
        body: 0x0045_6640,
    },
    ClassDestructor {
        selectors: &[42],
        deleting: 0x0040_1050,
        body: 0x0040_1070,
    },
    ClassDestructor {
        selectors: &[43],
        deleting: 0x0045_41f0,
        body: 0x0045_4210,
    },
    ClassDestructor {
        selectors: &[44],
        deleting: 0x0045_3ea0,
        body: 0x0045_3ec0,
    },
    ClassDestructor {
        selectors: &[45],
        deleting: 0x0045_bf60,
        body: 0x0045_bf80,
    },
    ClassDestructor {
        selectors: &[46],
        deleting: 0x0047_56a0,
        body: 0x0047_56c0,
    },
    ClassDestructor {
        selectors: &[47],
        deleting: 0x0047_fec0,
        body: 0x0047_fee0,
    },
    ClassDestructor {
        selectors: &[48],
        deleting: 0x0043_c810,
        body: 0x0043_c830,
    },
    ClassDestructor {
        selectors: &[49],
        deleting: 0x0047_63c0,
        body: 0x0047_63e0,
    },
    ClassDestructor {
        selectors: &[50],
        deleting: 0x0043_fe80,
        body: 0x0043_fea0,
    },
    ClassDestructor {
        selectors: &[51],
        deleting: 0x0040_4b70,
        body: 0x0040_4b90,
    },
    ClassDestructor {
        selectors: &[55],
        deleting: 0x0047_3fe0,
        body: 0x0047_4000,
    },
    ClassDestructor {
        selectors: &[56],
        deleting: 0x0041_10e0,
        body: 0x0048_54b0,
    },
    ClassDestructor {
        selectors: &[59, 73, 82],
        deleting: 0x0040_57b0,
        body: 0x0040_98b0,
    },
    ClassDestructor {
        selectors: &[60],
        deleting: 0x0048_0860,
        body: 0x0048_0880,
    },
    ClassDestructor {
        selectors: &[61],
        deleting: 0x0041_2970,
        body: 0x0041_2990,
    },
    ClassDestructor {
        selectors: &[62],
        deleting: 0x0046_3fd0,
        body: 0x0046_3ff0,
    },
    ClassDestructor {
        selectors: &[66],
        deleting: 0x0040_5190,
        body: 0x0040_51b0,
    },
    ClassDestructor {
        selectors: &[67],
        deleting: 0x0048_6570,
        body: 0x0048_6590,
    },
    ClassDestructor {
        selectors: &[68],
        deleting: 0x0046_1ba0,
        body: 0x0046_1bd0,
    },
    ClassDestructor {
        selectors: &[70],
        deleting: 0x0046_e7f0,
        body: 0x0046_e810,
    },
    ClassDestructor {
        selectors: &[72],
        deleting: 0x0046_fa20,
        body: 0x0046_fa40,
    },
    ClassDestructor {
        selectors: &[77, 79, 80],
        deleting: 0x0040_8ff0,
        body: 0x0040_7490,
    },
    ClassDestructor {
        selectors: &[78],
        deleting: 0x0040_6730,
        body: 0x0040_6750,
    },
    ClassDestructor {
        selectors: &[81],
        deleting: 0x0040_9290,
        body: 0x0040_92b0,
    },
    ClassDestructor {
        selectors: &[83],
        deleting: 0x0043_c320,
        body: 0x0043_c340,
    },
    ClassDestructor {
        selectors: &[84],
        deleting: 0x0043_c0a0,
        body: 0x0043_c0c0,
    },
    ClassDestructor {
        selectors: &[85],
        deleting: 0x0040_5f90,
        body: 0x0040_5fb0,
    },
    ClassDestructor {
        selectors: &[87],
        deleting: 0x0048_c760,
        body: 0x0048_c780,
    },
    ClassDestructor {
        selectors: &[88],
        deleting: 0x0044_38d0,
        body: 0x0044_38f0,
    },
    ClassDestructor {
        selectors: &[90],
        deleting: 0x0046_26c0,
        body: 0x0046_26e0,
    },
    ClassDestructor {
        selectors: &[91],
        deleting: 0x0040_2880,
        body: 0x0040_28a0,
    },
    ClassDestructor {
        selectors: &[93],
        deleting: 0x0044_0050,
        body: 0x0044_0070,
    },
];

#[cfg(test)]
#[path = "class_destructors_tests.rs"]
mod tests;
