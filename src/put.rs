use resolved_shared::{
    DATA_OFFSET, LEN_OFFSET, MemoryLimitExceeded, MsgPacket, SIZE, ShmemData, ShmemDataError,
    TYPE_OFFSET,
};

use crate::{Error, Script, packet::ShmemClient, script::ArgData};

#[derive(Debug)]
pub struct ShmemPut<'s> {
    shmem: &'s mut ShmemClient,
    cursor: usize,
}

impl<'s> ShmemPut<'s> {
    /// Creates a new writer to the data section of the shared memory with a cursor to continously write data
    pub fn new(shmem: &'s mut ShmemClient) -> Self {
        Self { shmem, cursor: 0 }
    }

    /// Sets the specific packet byte to `packet`
    pub fn set_packet(&mut self, packet: MsgPacket) {
        unsafe {
            let ptr = self.shmem.ptr().add(TYPE_OFFSET);
            std::ptr::write_volatile(ptr, packet as u8);
        }
    }

    /// writes data to the data and increments a cursor without transfering memory ownership to the sibling
    pub fn put_data(&mut self, data: &[u8]) -> Result<(), ShmemDataError> {
        let ptr_offset = DATA_OFFSET + self.cursor;
        if ptr_offset > (SIZE - DATA_OFFSET) {
            return Err(ShmemDataError::Memory(MemoryLimitExceeded(
                SIZE - DATA_OFFSET,
                ptr_offset,
            )));
        }

        let len = data.len();

        unsafe {
            let ptr = self.shmem.ptr().add(ptr_offset);
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, len);
        }

        self.cursor += len;
        Ok(())
    }

    /// Once all data has been written this writes the length of the data written to the length bytes of the shared memory
    pub fn finish(self) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "put_data and its len checks enforce that this fits"
        )]
        let total_data_len = self.cursor as u16;

        let len_size = size_of::<u16>();
        let len_be = (total_data_len).to_be_bytes();

        unsafe {
            let ptr = self.shmem.ptr().add(LEN_OFFSET);

            std::ptr::copy_nonoverlapping(len_be.as_ptr(), ptr, len_size);
        }

        self.cursor
    }

    pub fn put_u8(&mut self, v: u8) -> Result<(), ShmemDataError> {
        self.put_data(&[v])?;
        Ok(())
    }

    pub fn put_u32(&mut self, v: u32) -> Result<(), ShmemDataError> {
        self.put_data(&v.to_be_bytes())?;
        Ok(())
    }

    pub fn put_u64(&mut self, v: u64) -> Result<(), ShmemDataError> {
        self.put_data(&v.to_be_bytes())?;
        Ok(())
    }

    pub fn put_string(&mut self, s: &str) -> Result<(), Error> {
        self.put_u32(u32::try_from(s.len())?)?;
        self.put_data(s.as_bytes())?;
        Ok(())
    }

    /// Packs the [`Script`] into a serialized format.  
    ///
    /// `[if_with:u8,ref_id_if_with]`,
    /// `[str_len:u32;lua_script_str]`,
    /// `[args_len:u32]`,
    ///     `[arg_type:u8;arg_data]`,
    ///
    /// where `arg_data` can be just a value, a u64 for a ref, a len-prefixed string+value or len-prefixed string+u64
    pub fn put_script(&mut self, script: &Script) -> Result<(), Error> {
        self.put_u8(u8::from(script.with.is_some()))?;
        if let Some(item) = script.with {
            self.put_u64(item.id())?;
        }

        self.put_string(&script.lua)?;
        self.put_u32(u32::try_from(script.args.len())?)?;

        for arg in &script.args {
            self.put_u8(arg.arg_type())?;

            match arg {
                ArgData::Arg(arg) => {
                    self.put_u32(u32::try_from(arg.len())?)?;
                    self.put_data(arg)?;
                }
                ArgData::ArgRef(arg) => self.put_u64(arg.id())?,
                ArgData::NamedArg { key, value } => {
                    self.put_string(key)?;
                    self.put_u32(u32::try_from(value.len())?)?;
                    self.put_data(value)?;
                }
                ArgData::NamedArgRef { key, value } => {
                    self.put_string(key)?;
                    self.put_u64(value.id())?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use resolved_shared::{ShmemConf, shmem_struct};

    use super::*;

    fn mem_path() -> PathBuf {
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(".shared_memory_{}", fastrand::u64(..)))
    }

    shmem_struct!(ShmemModule, (Module => Client));
    impl ShmemModule {
        pub fn new(path: impl AsRef<Path>) -> Self {
            let _schmem = ShmemConf::new().flink(path).open().unwrap();
            Self {
                ptr: _schmem.as_ptr(),
                _schmem,
            }
        }
    }

    fn new_pair() -> (ShmemClient, ShmemModule) {
        let path = mem_path();
        let client = ShmemClient::new(&path).unwrap();
        let module = ShmemModule::new(&path);
        (client, module)
    }

    /// Asserts that an `expr` *(which returns a [`Result`])* matches a `pat`
    macro_rules! assert_error {
        ($err:pat = $run:expr, $($arg:tt)+) => {
            #[allow(irrefutable_let_patterns, reason = "this basically becomes a err() check, but could allow nested values to be checked")]
            let $err = $run.err().expect("Expected an Err(_), got an Ok(_) value") else {
                panic!($($arg)+);
            };
        };
        ($err:pat = $run:expr $(,)?) => {
            assert_error!($err = $run, "Got the wrong error type");
        };
    }

    #[test]
    fn new() -> Result<(), Error> {
        let (client, _) = new_pair();
        // client owner byte is 0, and since all is 0 it starts with the ownership
        assert!(client.check_owner().is_ok());
        // all data in shmem are 0
        assert_eq!([0, 0, 0, 0], client.get_handle());
        assert_eq!(0, client.get_len());

        Ok(())
    }

    #[test]
    fn synced() -> Result<(), Error> {
        let (client, module) = new_pair();

        client.write_data(&[55])?;

        assert_eq!([55], module.read_data()?);

        Ok(())
    }

    #[test]
    fn wrong_owner() -> Result<(), Error> {
        let (_, module) = new_pair();

        // if we dont write anything the owner stays at the client, which is invalid for our module
        assert_error!(
            ShmemDataError::Owner(_) = module.read_data(),
            "Expected OwnerError"
        );

        Ok(())
    }

    #[test]
    fn handle() -> Result<(), Error> {
        let (client, module) = new_pair();

        client.set_handle([1, 2, 3, 4]);
        client.set_owner(ShmemClient::SIBLING_ID);

        assert_eq!([1, 2, 3, 4], module.get_handle());

        Ok(())
    }

    #[test]
    fn len() -> Result<(), Error> {
        let (client, module) = new_pair();

        client.write_data(&[0, 1, 0, 1, 1, 0, 0])?;

        assert_eq!(7, module.get_len());

        Ok(())
    }

    #[test]
    fn exceeded_memory() -> Result<(), Error> {
        let (client, _) = new_pair();

        assert_error!(MemoryLimitExceeded(_, _) = client.set_len(usize::MAX));
        assert_error!(MemoryLimitExceeded(_, _) = client.set_len(SIZE - DATA_OFFSET));

        Ok(())
    }

    #[test]
    fn round_trip() -> Result<(), Error> {
        let (client, module) = new_pair();

        client.write_data(&[1, 2, 3, 4, 5])?;

        let data = module.read_data()?;
        let rev: Vec<u8> = data.iter().map(|x| *x).rev().collect();
        module.write_data(&rev)?;

        assert_eq!([5, 4, 3, 2, 1], client.read_data()?);

        Ok(())
    }
}
