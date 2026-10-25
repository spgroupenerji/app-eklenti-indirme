use crate::util::STATE_EXT;
use std::io;
use std::path::{Path, PathBuf};

const JURNAL_MAGIC: u64 = 0x54495441_4E535401;

#[derive(Debug, thiserror::Error)]
pub enum DepoHatasi {
    #[error("Dosya açılamadı: {0}")]
    Acma(io::Error),
    #[error("Dosya boyutu ayarlanamadı: {0}")]
    Boyut(io::Error),
    #[error("Diske yazılamadı: {0}")]
    Yazma(io::Error),
}

fn seyrek_isaretle(dosya: &std::fs::File, toplam: u64) -> Result<(), io::Error> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::HANDLE;
        use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
        use windows_sys::Win32::System::IO::DeviceIoControl;
        let _ = toplam;
        let tutamak = dosya.as_raw_handle() as HANDLE;
        let mut donen: u32 = 0;
        let sonuc = unsafe {
            DeviceIoControl(
                tutamak,
                FSCTL_SET_SPARSE,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut donen,
                std::ptr::null_mut(),
            )
        };
        if sonuc == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let ret = unsafe { libc::fallocate(dosya.as_raw_fd(), 0, 0, toplam as libc::off_t) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

pub struct HedefDosya {
    pub dosya: std::fs::File,
    pub yol: PathBuf,
}

impl HedefDosya {
    pub fn olustur(yol: &Path, toplam: u64) -> Result<Self, DepoHatasi> {
        let std_dosya = std::fs::File::options()
            .create(true)
            .read(true)
            .write(true)
            .open(yol)
            .map_err(DepoHatasi::Acma)?;
        if seyrek_isaretle(&std_dosya, toplam).is_err() {
            // NTFS dışı dosya sistemlerinde sessiz geri düşme
        }
        std_dosya.set_len(toplam).map_err(DepoHatasi::Boyut)?;
        Ok(Self {
            dosya: std_dosya,
            yol: yol.to_path_buf(),
        })
    }

    pub async fn yaz_ofset(&self, ofset: u64, veri: &[u8]) -> Result<(), DepoHatasi> {
        let dosya = &self.dosya;
        tokio::task::block_in_place(move || {
            let mut yazilan = 0usize;
            while yazilan < veri.len() {
                #[cfg(windows)]
                let n = std::os::windows::fs::FileExt::seek_write(
                    dosya,
                    &veri[yazilan..],
                    ofset + yazilan as u64,
                )
                .map_err(DepoHatasi::Yazma)?;
                #[cfg(unix)]
                {
                    std::os::unix::fs::FileExt::write_all_at(
                        dosya,
                        &veri[yazilan..],
                        ofset + yazilan as u64,
                    )
                    .map_err(DepoHatasi::Yazma)?;
                    let n = veri.len() - yazilan;
                }
                yazilan += n;
            }
            Ok(())
        })
    }

    pub async fn senkron(&self) -> Result<(), DepoHatasi> {
        let dosya = &self.dosya;
        tokio::task::block_in_place(move || dosya.sync_data().map_err(DepoHatasi::Yazma))
    }
}

pub struct Jurnal {
    dosya: std::fs::File,
    pub yol: PathBuf,
}

impl Jurnal {
    pub fn dizi_yolu(hedef: &Path) -> PathBuf {
        let mut s = hedef.as_os_str().to_os_string();
        s.push(format!(".{STATE_EXT}"));
        PathBuf::from(s)
    }

    pub fn ac(hedef: &Path, toplam: u64) -> io::Result<Self> {
        let yol = Self::dizi_yolu(hedef);
        let mut dosya = std::fs::File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&yol)?;
        if dosya.metadata()?.len() == 0 {
            use std::io::Write;
            dosya.write_all(&JURNAL_MAGIC.to_le_bytes())?;
            dosya.write_all(&toplam.to_le_bytes())?;
            dosya.write_all(&0u32.to_le_bytes())?;
            dosya.sync_all()?;
        }
        Ok(Self { dosya, yol })
    }

    pub fn yukle(hedef: &Path) -> io::Result<Option<(u64, Vec<(u64, u64)>)>> {
        let yol = Self::dizi_yolu(hedef);
        let veri = match std::fs::read(&yol) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        if veri.len() < 20 {
            return Ok(None);
        }
        let magic = u64::from_le_bytes(veri[0..8].try_into().unwrap());
        if magic != JURNAL_MAGIC {
            return Ok(None);
        }
        let toplam = u64::from_le_bytes(veri[8..16].try_into().unwrap());
        let adet = u32::from_le_bytes(veri[16..20].try_into().unwrap()) as usize;
        if veri.len() < 20 + adet * 16 {
            return Ok(None);
        }
        let mut araliklar = Vec::with_capacity(adet);
        for i in 0..adet {
            let b = 20 + i * 16;
            let bas = u64::from_le_bytes(veri[b..b + 8].try_into().unwrap());
            let bit = u64::from_le_bytes(veri[b + 8..b + 16].try_into().unwrap());
            if bas < bit && bit <= toplam {
                araliklar.push((bas, bit));
            }
        }
        Ok(Some((toplam, araliklar)))
    }

    pub fn aralik_ekle(&mut self, bas: u64, bit: u64) -> io::Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        let konum = self.dosya.metadata()?.len();
        self.dosya.seek(SeekFrom::Start(konum))?;
        self.dosya.write_all(&bas.to_le_bytes())?;
        self.dosya.write_all(&bit.to_le_bytes())?;
        let adet = ((konum - 20) / 16 + 1) as u32;
        self.dosya.seek(SeekFrom::Start(16))?;
        self.dosya.write_all(&adet.to_le_bytes())?;
        self.dosya.seek(SeekFrom::End(0))?;
        self.dosya.sync_all()
    }

    pub fn kaldir(&self) {
        let _ = std::fs::remove_file(&self.yol);
    }
}

pub fn aralik_birlestir(mut liste: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    liste.sort_unstable();
    let mut birlesik: Vec<(u64, u64)> = Vec::new();
    for (bas, bit) in liste {
        match birlesik.last_mut() {
            Some(son) if bas <= son.1 => {
                if bit > son.1 {
                    son.1 = bit;
                }
            }
            _ => birlesik.push((bas, bit)),
        }
    }
    birlesik
}

pub fn tumleyen(toplam: u64, birlesik: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut kalan = Vec::new();
    let mut imlec = 0u64;
    for (bas, bit) in birlesik {
        if *bas > imlec {
            kalan.push((imlec, *bas));
        }
        imlec = imlec.max(*bit);
    }
    if imlec < toplam {
        kalan.push((imlec, toplam));
    }
    kalan
}
