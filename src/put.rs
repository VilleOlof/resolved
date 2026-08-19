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
        // put_data and its len checks enforce that this fits
        #[allow(clippy::cast_possible_truncation)]
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
