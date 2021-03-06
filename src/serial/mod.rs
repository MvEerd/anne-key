pub mod bluetooth_usart;
pub mod led_usart;

use nb;
use super::protocol::MsgType;
use core::marker::PhantomData;


pub struct Serial<'a, USART>
    where USART: DmaUsart
{
    pub usart: USART,
    send_buffer: &'a mut[u8; 0x20],
    pub send_buffer_pos: u16,
}

pub trait DmaUsart {
    // TODO: naming of these isn't quite perfect
    // TODO: better types?
    fn is_receive_pending(&mut self) -> bool;
    fn receive(&mut self, length: u16, buffer: u32);
    fn is_send_ready(&mut self) -> bool;
    fn send(&mut self, buffer: u32, len: u16);
    fn ack_wakeup(&mut self);
    fn tx_interrupt(&mut self);
}

enum ReceiveStage {
    Header,
    Body,
}

const HEADER_SIZE: u16 = 2;

pub struct Transfer {
    pub buffer: &'static mut [u8; 0x20],
    receive_stage: ReceiveStage,
    //_usart: PhantomData<&USART>,
}

impl Transfer
{
    pub fn poll<USART>(&mut self, usart: &mut USART) -> nb::Result<(), !>
      where USART: DmaUsart
    {
        if usart.is_receive_pending() {
            match self.receive_stage {
                ReceiveStage::Header => {
                    self.receive_stage = ReceiveStage::Body;
                    usart.receive(u16::from(self.buffer[1]),
                        self.buffer.as_mut_ptr() as u32 + u32::from(HEADER_SIZE));

                    return Err(nb::Error::WouldBlock)
                }
                ReceiveStage::Body => {
                    return Ok(())
                }
            }
        } else {
            return Err(nb::Error::WouldBlock)
        }
    }

    pub fn finish(self) -> &'static mut [u8; 0x20] {
        self.buffer
    }
}

impl<'a, USART> Serial<'a, USART>
    where USART: DmaUsart
{
    pub fn new(usart: USART, send_buffer: &'a mut [u8; 0x20])
        -> Serial<'a, USART> {
        Serial {
            usart: usart,
            send_buffer: send_buffer,
            send_buffer_pos: 0,
        }
    }

    pub fn receive(&mut self, recv_buffer: &'static mut [u8; 0x20]) -> Transfer
    {
        self.usart.receive(HEADER_SIZE, recv_buffer.as_mut_ptr() as u32);

        Transfer { buffer: recv_buffer, receive_stage: ReceiveStage::Header }
    }

    pub fn send(
        &mut self,
        message_type: MsgType,
        operation: u8, // TODO: make this typed?
        data: &[u8]) -> nb::Result<(), !> {
        let tx_len = 3 + data.len() as u16;
        if self.usart.is_send_ready() && self.send_buffer_pos + tx_len < self.send_buffer.len() as u16 {
            // TODO: put this into buffer, but then increase buffer offset
            // keep counter, use counter when calling send()
            let pos = self.send_buffer_pos as usize;
            self.send_buffer[pos] = message_type as u8;
            self.send_buffer[pos + 1] = 1 + data.len() as u8;
            self.send_buffer[pos + 2] = operation;
            self.send_buffer[pos + 3..pos + tx_len as usize].clone_from_slice(data);

            self.send_buffer_pos += tx_len;

            self.usart.send(self.send_buffer.as_mut_ptr() as u32, self.send_buffer_pos);

            return Ok(())
        } else {
            return Err(nb::Error::WouldBlock)
        }
    }

    pub fn tx_interrupt(&mut self) {
        self.send_buffer_pos = 0;
        self.usart.tx_interrupt();
    }
}
