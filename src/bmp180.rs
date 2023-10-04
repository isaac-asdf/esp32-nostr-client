use esp32_hal::{i2c::I2C, peripherals::I2C0, prelude::*};
use esp_println::println;

const BMP_I2C_ADDRESS: u8 = 0x77;
const CAL_DATA_START: u8 = 0xaa;
const READ_DATA_START: u8 = 0xf6;

#[derive(Debug, Default)]
pub struct BmpCalData {
    _ac1: i16,
    _ac2: i16,
    _ac3: i16,
    _ac4: u16,
    ac5: u16,
    ac6: u16,
    _b1: i16,
    _b2: i16,
    _mb: i16,
    mc: i16,
    md: i16,
}

impl From<[u8; 22]> for BmpCalData {
    fn from(value: [u8; 22]) -> Self {
        BmpCalData {
            _ac1: i16::from_be_bytes([value[0], value[1]]),
            _ac2: i16::from_be_bytes([value[2], value[3]]),
            _ac3: i16::from_be_bytes([value[4], value[5]]),
            _ac4: u16::from_be_bytes([value[6], value[7]]),
            ac5: u16::from_be_bytes([value[8], value[9]]),
            ac6: u16::from_be_bytes([value[10], value[11]]),
            _b1: i16::from_be_bytes([value[12], value[13]]),
            _b2: i16::from_be_bytes([value[14], value[15]]),
            _mb: i16::from_be_bytes([value[16], value[17]]),
            mc: i16::from_be_bytes([value[18], value[19]]),
            md: i16::from_be_bytes([value[20], value[21]]),
        }
    }
}

pub struct Bmp180<'d> {
    i2c: I2C<'d, I2C0>,
    cal_data: BmpCalData,
}

impl<'d> Bmp180<'d> {
    pub fn new(mut i2c: I2C<'d, I2C0>) -> Self {
        let mut data = [0u8; 22];
        i2c.write_read(BMP_I2C_ADDRESS, &[CAL_DATA_START], &mut data)
            .ok();
        let cal_data: BmpCalData = data.into();
        Bmp180 {
            i2c: i2c,
            cal_data: cal_data,
        }
    }

    /// must be called prior to get_temperature(), with a 4.5ms delay between the two
    pub fn start_temp_read(&mut self) -> bool {
        self.i2c.write(BMP_I2C_ADDRESS, &[0xf4, 0x2e]).is_ok()
    }

    pub fn get_temperature(&mut self) -> f32 {
        let mut data: [u8; 2] = [0; 2];
        self.i2c
            .write_read(BMP_I2C_ADDRESS, &[READ_DATA_START], &mut data)
            .ok();
        // let ut: u16 = u16::from_be_bytes([data[0], data[1]]);
        let ut: u16 = ((data[0] as i32) << 8) as u16 | data[1] as u16;

        // let ut: i32 = i32::from(data[0] << 8) + i32::from(data[1].to_be());
        let x1: i32 =
            ((ut as i32 - self.cal_data.ac6 as i32) as i64 * self.cal_data.ac5 as i64 >> 15) as i32; // Note: X>>15 == X/(pow(2,15))
        let x2: i32 = ((self.cal_data.mc as i32) << 11) / (x1 + self.cal_data.md as i32); // Note: X<<11 == X<<(pow(2,11))
        let b5: i32 = x1 + x2;
        let t: i32 = (b5 + 8) >> 4;
        t as f32 / 10.
    }
}
