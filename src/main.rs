#![feature(const_fn)]
#![feature(proc_macro)]
#![feature(non_exhaustive)]
#![no_std]

extern crate bare_metal;
extern crate cortex_m;
extern crate cortex_m_rtfm as rtfm;
extern crate cortex_m_semihosting;
extern crate embedded_hal;
extern crate stm32l151;
extern crate stm32l151_hal as hal;

#[macro_use]
mod debug;

#[macro_use]
mod action;
mod bluetooth;
mod clock;
mod hidreport;
mod keyboard;
mod keycodes;
mod keymatrix;
mod layout;
mod led;
mod protocol;
mod serial;

use rtfm::{app, Threshold};
use hal::dma::DmaExt;
use hal::gpio::GpioExt;

use bluetooth::Bluetooth;
use keyboard::Keyboard;
use keymatrix::KeyMatrix;
use led::Led;
use serial::Serial;
use serial::bluetooth_usart::BluetoothUsart;
use serial::led_usart::LedUsart;

app! {
    device: stm32l151,

    resources: {
        static KEYBOARD: Keyboard = Keyboard::new();
        static KEY_MATRIX: KeyMatrix;
        static BLUETOOTH_BUFFERS: [[u8; 0x10]; 2] = [[0; 0x10]; 2];
        static BLUETOOTH: Bluetooth<'static>;
        static LED_BUFFERS: [[u8; 0x10]; 2] = [[0; 0x10]; 2];
        static LED: Led<'static>;
        static SYST: stm32l151::SYST;
        static EXTI: stm32l151::EXTI;
    },

    init: {
        resources: [BLUETOOTH_BUFFERS, LED_BUFFERS],
    },

    tasks: {
        SYS_TICK: {
            path: tick,
            resources: [BLUETOOTH, LED, KEY_MATRIX, SYST, KEYBOARD],
        },
        DMA1_CHANNEL2: {
            path: led::tx,
            resources: [LED],
        },
        DMA1_CHANNEL3: {
            path: led::rx,
            resources: [LED],
        },
        DMA1_CHANNEL6: {
            path: bluetooth::rx,
            resources: [BLUETOOTH, KEY_MATRIX],
        },
        DMA1_CHANNEL7: {
            path: bluetooth::tx,
            resources: [BLUETOOTH],
        },
        EXTI9_5: {
            path: exti9_5,
            resources: [EXTI],
        },
    }
}

fn init(mut p: init::Peripherals, r: init::Resources) -> init::LateResources {
    // re-locate vector table to 0x80004000 because bootloader uses 0x80000000
    unsafe { p.core.SCB.vtor.write(0x4000) };

    let mut d = p.device;
    clock::init_clock(&d);
    clock::enable_tick(&mut p.core.SYST, 100_000);

    let dma = d.DMA1.split();
    let gpioa = d.GPIOA.split();
    let gpiob = d.GPIOB.split();
    let gpioc = d.GPIOC.split();

    let row_pins = (gpiob.pb9.pull_down(),
                    gpiob.pb8.pull_down(),
                    gpiob.pb7.pull_down(),
                    gpiob.pb6.pull_down(),
                    gpioa.pa0.pull_down());

    // TODO: make pin a generic trait, then iterate over list and call .into_output().pull_up()?
    let column_pins = (gpioa.pa5.into_output().pull_up(),
                       gpioa.pa6.into_output().pull_up(),
                       gpioa.pa7.into_output().pull_up(),
                       gpiob.pb0.into_output().pull_up(),
                       gpiob.pb1.into_output().pull_up(),
                       gpiob.pb12.into_output().pull_up(),
                       gpiob.pb13.into_output().pull_up(),
                       gpiob.pb14.into_output().pull_up(),
                       gpioa.pa8.into_output().pull_up(),
                       gpioa.pa9.into_output().pull_up(),
                       gpioa.pa15.into_output().pull_up(),
                       gpiob.pb3.into_output().pull_up(),
                       gpiob.pb4.into_output().pull_up(),
                       gpiob.pb5.into_output().pull_up());

    let key_matrix = KeyMatrix::new(row_pins, column_pins);

    let led_usart = LedUsart::new(d.USART3, gpiob.pb10, gpiob.pb11, dma.3, dma.2, &mut d.RCC);
    let led_serial = Serial::new(led_usart, r.LED_BUFFERS);
    let led = Led::new(led_serial, gpioc.pc15);

    let bluetooth_usart = BluetoothUsart::new(d.USART2, gpioa.pa1, gpioa.pa2, gpioa.pa3, dma.6, dma.7, &mut d.RCC);
    let bluetooth_serial = Serial::new(bluetooth_usart, r.BLUETOOTH_BUFFERS);
    let bluetooth = Bluetooth::new(bluetooth_serial);

    init::LateResources {
        BLUETOOTH: bluetooth,
        KEY_MATRIX: key_matrix,
        LED: led,
        SYST: p.core.SYST,
        EXTI: d.EXTI,
    }
}

fn idle() -> ! {
    loop {
        rtfm::wfi();
    }
}

fn tick(_t: &mut Threshold, mut r: SYS_TICK::Resources) {
    r.KEY_MATRIX.sample(&r.SYST);
    r.KEYBOARD.process(&r.KEY_MATRIX.state, &mut r.BLUETOOTH, &mut r.LED);
}

fn exti9_5(_t: &mut Threshold, r: EXTI9_5::Resources) {
    // this (plus other exti) are key presses,
    // maybe use them instead of timer based scanning?

    // maybe only clear set bits? or ones from 9-5?
    unsafe { r.EXTI.pr.write(|w| w.bits(0xffff)) };
}
