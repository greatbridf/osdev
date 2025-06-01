extern crate alloc;

use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone)]
pub struct HartInfo {
    pub hart_id: usize,
    pub compatible: Option<String>,
}

#[derive(Debug)]
pub struct CpuManager {
    pub num_harts: usize,
    pub harts: Vec<HartInfo>,
    pub boot_hart_id: usize,
}

impl CpuManager {
    pub fn from_fdt(dtb_addr: usize, boot_hart_id: usize) -> Self {
        let fdt = unsafe {
            fdt::Fdt::from_ptr(dtb_addr as *const u8)
                .expect("Failed to parse device tree from dtb_addr")
        };

        let mut harts_info = Vec::new();
        let mut num_harts = 0;

        if let Some(cpus_node) = fdt.find_node("/cpus") {
            for cpu_node in cpus_node.children() {
                let mut is_riscv_cpu = false;
                let compatible_string: Option<String> = 
                    if let Some(compatible_prop) = cpu_node
                        .properties()
                        .find(|p| p.name == "compatible")
                    {
                        if let Some(s) = compatible_prop.as_str() {
                            if s.starts_with("riscv,") {
                                is_riscv_cpu = true;
                            }
                            Some(String::from(s))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                if is_riscv_cpu {
                    let hart_id = if let Some(hart_id_prop) = cpu_node
                        .properties()
                        .find(|p| p.name == "hartid")
                    {
                        hart_id_prop.as_usize()
                            .expect("hartid property in CPU node is not a valid integer")
                    } else if let Some(reg_prop) = cpu_node
                        .properties()
                        .find(|p| p.name == "reg")
                    {
                        reg_prop.as_usize()
                            .expect("reg property in CPU node is not a valid integer for hartid")
                    } else {
                        panic!("CPU node {:?} does not have a 'hartid' or 'reg' property required for its ID", cpu_node.name);
                    };

                    let hart_info = HartInfo {
                        hart_id,
                        compatible: compatible_string,
                    };
                    harts_info.push(hart_info);
                    num_harts += 1;
                }
            }
        } else {
            panic!("Device Tree does not contain a /cpus node!");
        }

        harts_info.sort_by_key(|h| h.hart_id);

        Self {
            num_harts,
            harts: harts_info,
            boot_hart_id,
        }
    }
}

pub fn get_num_harts(dtb_addr: usize) -> usize {
    let fdt = unsafe {
        fdt::Fdt::from_ptr(dtb_addr as *const u8)
            .expect("Failed to parse device tree from dtb_addr")
    };
    let dtb_cpus = fdt.cpus();
    let mut num_harts = 0;

    for _cpu in dtb_cpus {
        num_harts += 1;
    }
    num_harts
}
