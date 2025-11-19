#[doc = r" The RTIC application module"] pub mod app
{
    #[doc =
    r" Always include the device crate which contains the vector table"] use
    hal :: stm32 as
    you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml; pub use
    rtic :: Monotonic as _;
    #[doc = r" Holds static methods for each monotonic."] pub mod monotonics
    {
        pub use MonoTimer :: now;
        #[doc =
        "This module holds the static implementation for `MonoTimer::now()`"]
        #[allow(non_snake_case)] pub mod MonoTimer
        {
            #[doc = r" Read the current time from this monotonic"] pub fn
            now() -> < super :: super :: MonoTimer as rtic :: Monotonic > ::
            Instant
            {
                rtic :: export :: interrupt ::
                free(| _ |
                {
                    use rtic :: Monotonic as _; if let Some(m) = unsafe
                    {
                        & mut * super :: super ::
                        __rtic_internal_MONOTONIC_STORAGE_MonoTimer.get_mut()
                    } { m.now() } else
                    {
                        < super :: super :: MonoTimer as rtic :: Monotonic > ::
                        zero()
                    }
                })
            }
        }
    } use super :: * ; use hal :: exti :: TriggerEdge;
    #[doc = r" User code from within the module"] defmt :: timestamp!
    ("{=u64:tms}", { monotonics::now().ticks() }); type MonoTimer = Systick <
    1000 > ; #[doc = r" User code end"]
    #[doc = " User provided init function"] #[inline(always)]
    #[allow(non_snake_case)] fn init(cx : init :: Context) ->
    (Shared, Local, init :: Monotonics)
    {
        defmt :: info! ("init"); let mut p = cx.device;
        p.DBGMCU.cr.modify(| _, w |
        {
            w.dbg_sleep().set_bit().dbg_stop().set_bit().dbg_standby().set_bit()
        }); let mut rcc =
        p.RCC.freeze(Config ::
        pll(hal :: rcc :: PLLSource :: HSE(8.mhz()), hal :: rcc :: PLLMul ::
        Mul6, hal :: rcc :: PLLDiv :: Div3,)); defmt :: info! ("rcc freeze");
        rcc.enable_power(); defmt :: info! ("enable_power"); let mut rtc = Rtc
        :: new(p.RTC, & mut p.PWR); defmt :: info! ("rtc"); let mono = Systick
        :: new(cx.core.SYST, 16_000_000); defmt :: info! ("mono"); let
        hardware :: Pins
        {
            lcd_rs, lcd_rw, lcd_e, lcd_d4, lcd_d5, lcd_d6, lcd_d7, lcd_on,
            lcd_led, button_set, button_enter, button_down, button_up,
            spi_sck, spi_miso, spi_mosi, mco, osc_en, tx, rx, mut rs_power_en,
            memory_en, memory_hold, memory_wp, tdc1000_en, tdc1000_cs,
            tdc1000_res, tdc7200_en, tdc7200_cs, tdc7200_int, sw_en, sw_a0,
            sw_a1, photo_r, ext_in, ext_out, gpio_power,
        } = hardware :: Pins ::
        new(p.GPIOA, p.GPIOB, p.GPIOC, p.GPIOD, p.GPIOH); let _ = osc_en; let
        _ = tdc7200_en; let _ = tdc7200_cs; let _ = tdc7200_int; let _ =
        sw_en; let _ = sw_a0; let _ = sw_a1; let _ = ext_in; let _ = ext_out;
        rcc.configure_mco(MCOSel :: Hse, MCODiv :: Div1, mco); let hd44780 :
        LcdHardware = LcdHardware ::
        new(lcd_rs, lcd_e, lcd_d4, lcd_d5, lcd_d6, lcd_d7, lcd_rw); let mut
        lcd = Lcd :: new(hd44780, lcd_on, lcd_led); lcd.init(); let mut adc =
        p.ADC.adc(& mut rcc);
        adc.set_precision(hal :: adc :: Precision :: B_12); let keyboard =
        Keyboard :: new(button_set, button_enter, button_down, button_up);
        spi_sck.set_alt_mode(AltMode :: SPI1_2);
        spi_miso.set_alt_mode(AltMode :: SPI1_2);
        spi_mosi.set_alt_mode(AltMode :: SPI1_2); let spi =
        p.SPI2.spi((spi_sck, spi_miso, spi_mosi), spi :: MODE_0, 16.mhz(), &
        mut rcc,); let bus = shared_bus_rtic :: new! (spi, BusType); defmt ::
        info! ("e25x"); let eeprom25x = Eeprom25x ::
        new(bus.acquire(), memory_en, memory_wp, memory_hold).unwrap(); let
        mut storage = microchip_eeprom_25lcxx :: Storage :: new(eeprom25x);
        let mut opt = Options :: load(& mut storage).unwrap(); let reg =
        [0x31_u8, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0, 0,
        0, 0, 0, 0,]; opt.set_tdc7200_regs(u128 :: from_le_bytes(reg)); defmt
        :: info! ("opt: {:x}", opt.into_bytes()); let mut tdc1000 = TDC1000 ::
        new(bus.acquire(), tdc1000_cs, tdc1000_res, tdc1000_en); let mut cfg =
        Config0 :: default(); cfg.set_num_tx(31);
        cfg.set_tx_freq_div(FrequencyDividerForTx :: Div16); let bytes =
        cfg.into_bytes(); tdc1000.set_config0(cfg).ok(); defmt :: info!
        ("tdc1000_regs: {:x}", bytes); let mut asd = HourHistory ::
        new(& mut storage).unwrap(); defmt :: info!
        ("read data.size: {:?} {:?} {:?}", asd.data.size(),
        asd.first_stored_timestamp(), asd.last_stored_timestamp());
        rs_power_en.set_low().unwrap(); let mut serial =
        p.USART1.usart((tx, rx), serial :: Config ::
        default().baudrate(hal :: time :: Bps(112500)), & mut rcc,).unwrap();
        writeln! (serial, "Hello world\r").unwrap(); block!
        (serial.flush()).ok(); defmt :: info!
        ("{}", compile_time::datetime_str!()); defmt :: info!
        ("{}", compile_time::rustc_version_str!()); let datetime =
        compile_time :: datetime! ().saturating_add(Duration :: HOUR * 2); let
        asd = datetime.unix_timestamp(); defmt :: info!
        ("unix_timestamp {}", asd);
        rtc.set_datetime(& PrimitiveDateTime ::
        new(datetime.date(), datetime.time())).unwrap(); defmt :: info!
        ("rtc init"); rtc.enable_wakeup(5);
        rtc.listen(& mut p.EXTI, Event :: Wakeup); let mut exti = ExtiExt ::
        new(p.EXTI); rcc.apb2enr.modify(| _, w | w.syscfgen().set_bit());
        exti.listen_gpio(& mut p.SYSCFG, 1, 6, TriggerEdge :: Falling);
        exti.listen_gpio(& mut p.SYSCFG, 1, 7, TriggerEdge :: Falling);
        exti.listen_gpio(& mut p.SYSCFG, 1, 8, TriggerEdge :: Falling);
        exti.listen_gpio(& mut p.SYSCFG, 1, 9, TriggerEdge :: Falling); let
        mut timer = p.TIM2.timer(20.hz(), & mut rcc); timer.listen(); let mut
        ui_timer = p.TIM3.timer(10.hz(), & mut rcc); ui_timer.listen(); let
        power = Power :: new(gpio_power, rcc, p.PWR, cx.core.SCB); app_request
        :: spawn(AppRequest :: DeepSleep).ok(); defmt :: info! ("init end");
        (Shared
        {
            power, rtc, lcd, hour_history : HourHistory ::
            new(& mut storage).unwrap(), day_history : DayHistory ::
            new(& mut storage).unwrap(), month_history : MonthHistory ::
            new(& mut storage).unwrap(), storage, app : App :: default(), ui :
            Viewport :: default(),
        }, Local { keyboard, timer, ui_timer, handle : None, adc, photo_r, },
        init :: Monotonics(mono),)
    } #[doc = " User HW task: rtc_timer"] #[allow(non_snake_case)] fn
    rtc_timer(ctx : rtc_timer :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; defmt ::
        info! ("rtc_timer"); let rtc_timer :: SharedResources { power, rtc } =
        ctx.shared;
        (power,
        rtc).lock(| power, rtc |
        { rtc.unpend(Event :: Wakeup); power.exit_sleep(); }); app_request ::
        spawn(AppRequest :: Process).ok();
    } #[doc = " User HW task: exti9_5"] #[allow(non_snake_case)] fn
    exti9_5(ctx : exti9_5 :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; let mut
        power = ctx.shared.power; ExtiExt :: unpend(6); ExtiExt :: unpend(7);
        ExtiExt :: unpend(8); ExtiExt :: unpend(9);
        power.lock(| power | { power.active(); power.exit_sleep(); });
    } #[doc = " User HW task: timer"] #[allow(non_snake_case)] fn
    timer(ctx : timer :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; let timer
        :: SharedResources { mut power, lcd, app, ui, } = ctx.shared;
        ctx.local.timer.clear_irq(); let is_active =
        power.lock(| power | power.is_active()); if is_active
        {
            let event = ctx.local.keyboard.read_ui_keys();
            (app, lcd,
            ui).lock(| app, lcd, ui |
            {
                if let Some(event) = event
                {
                    app.handle_event(ui.event(event)).map(app_request :: spawn);
                } if lcd.init()
                { defmt :: info! ("lcd init"); ui.invalidate(); }
                ui.update(app); ui.render(lcd);
            }); if event.is_some()
            {
                if let Some(h) = ctx.local.handle.take() { h.cancel().ok(); }
                * ctx.local.handle = app_request ::
                spawn_after(Power :: IDLE_TIMEOUT.secs(), AppRequest ::
                DeepSleep).ok();
            }
        }
    } #[doc = " User HW task: ui_timer"] #[allow(non_snake_case)] fn
    ui_timer(ctx : ui_timer :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; let
        ui_timer :: SharedResources { mut power, lcd, app, ui, rtc, } =
        ctx.shared; ctx.local.ui_timer.clear_irq(); let is_active =
        power.lock(| power | power.is_active()); if is_active
        {
            (app, lcd, ui,
            rtc).lock(| app, lcd, ui, rtc |
            {
                app.datetime = rtc.get_datetime(); if lcd.init()
                { ui.invalidate(); } ui.update(app); ui.render(lcd);
            }); let chan_val : u16 =
            ctx.local.adc.read(ctx.local.photo_r).unwrap(); if chan_val < 500
            && * ctx.local.led
            {
                * ctx.local.led = false; app_request ::
                spawn(AppRequest :: LcdLed(* ctx.local.led)).ok();
            } else if chan_val > 500 && ! * ctx.local.led
            {
                * ctx.local.led = true; app_request ::
                spawn(AppRequest :: LcdLed(* ctx.local.led)).ok();
            }
        }
    } #[doc = " User SW task app_request"] #[allow(non_snake_case)] fn
    app_request(ctx : app_request :: Context, req : AppRequest)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; let
        app_request :: SharedResources
        {
            power, mut lcd, mut rtc, mut app, hour_history, day_history,
            month_history, mut storage,
        } = ctx.shared; match req
        {
            AppRequest :: Process =>
            {
                defmt :: info! ("Process"); let datetime =
                rtc.lock(| rtc | { return rtc.get_datetime(); }); let
                (hour_flow, day_flow, month_flow) =
                app.lock(| app |
                {
                    let mut rng = Pcg32 ::
                    seed_from_u64(monotonics :: now().ticks()); app.flow =
                    rng.next_u32() as f32 / 1_000_000.0; defmt :: info!
                    ("flow: {}", app.flow); app.hour_flow += app.flow;
                    app.day_flow += app.flow; app.month_flow += app.flow;
                    (app.hour_flow, app.day_flow, app.month_flow)
                }); if datetime.time().second() < 5
                {
                    let timestamp = datetime.as_utc().unix_timestamp(); if
                    datetime.time().minute() == 0
                    {
                        if let Err(_e) =
                        (hour_history, & mut
                        storage).lock(| hour_history, storage |
                        {
                            hour_history.add(storage, hour_flow as i32, timestamp as
                            u32)
                        }) { defmt :: error! ("Failed to log hour flow:"); } else
                        {
                            defmt :: info!
                            ("Hour flow logged: {} at {}", hour_flow, timestamp);
                        } if datetime.time().hour() == 0
                        {
                            if let Err(_e) =
                            (day_history, & mut
                            storage).lock(| day_history, storage |
                            {
                                day_history.add(storage, day_flow as i32, timestamp as u32)
                            }) { defmt :: error! ("Failed to log day flow:"); } else
                            {
                                defmt :: info!
                                ("Day flow logged: {} at {}", day_flow, timestamp);
                            } if datetime.date().day() == 1
                            {
                                if let Err(_e) =
                                (month_history, & mut
                                storage).lock(| month_history, storage |
                                {
                                    month_history.add(storage, month_flow as i32, timestamp as
                                    u32,)
                                }) { defmt :: error! ("Failed to log month flow:"); } else
                                {
                                    defmt :: info!
                                    ("Month flow logged: {} at {}", month_flow, timestamp);
                                }
                            }
                        }
                    }
                } app_request ::
                spawn_after(25_u64.millis(), AppRequest :: DeepSleep).ok();
            } AppRequest :: LcdLed(on) =>
            {
                defmt :: info! ("LcdLed {}", on);
                lcd.lock(| lcd | lcd.led(on));
            } AppRequest :: SetDateTime(dt) =>
            { rtc.lock(| rtc | rtc.set_datetime(& dt).ok()); } AppRequest ::
            DeepSleep =>
            {
                defmt :: info! ("DeepSleep");
                (power,
                lcd).lock(| power, lcd |
                { power.enter_sleep(| | { lcd.led_off(); lcd.off(); }); });
            } AppRequest :: SetHistory(history_type, timestamp) =>
            {
                defmt :: info! ("SetHistory"); match history_type
                {
                    HistoryType :: Hour =>
                    {
                        (app, hour_history,
                        storage).lock(| app, hour_history, storage |
                        {
                            if let Ok(Some(flow)) =
                            hour_history.find(storage, timestamp as u32)
                            { app.history_state.flow = Some(flow as f32); } else
                            { app.history_state.flow = None; }
                        });
                    } HistoryType :: Day =>
                    {
                        (app, day_history,
                        storage).lock(| app, day_history, storage |
                        {
                            if let Ok(Some(flow)) =
                            day_history.find(storage, timestamp as u32)
                            { app.history_state.flow = Some(flow as f32); } else
                            { app.history_state.flow = None; }
                        });
                    } HistoryType :: Month =>
                    {
                        (app, month_history,
                        storage).lock(| app, month_history, storage |
                        {
                            if let Ok(Some(flow)) =
                            month_history.find(storage, timestamp as u32)
                            { app.history_state.flow = Some(flow as f32); } else
                            { app.history_state.flow = None; }
                        });
                    }
                };
            }
        }
    } #[doc = " RTIC shared resource struct"] struct Shared
    {
        power : hardware :: Power, rtc : Rtc < hal :: rtc :: Lse > , lcd :
        Lcd, hour_history : HourHistory, day_history : DayHistory,
        month_history : MonthHistory, storage : MyStorage, app : App, ui :
        Viewport,
    } #[doc = " RTIC local resource struct"] struct Local
    {
        keyboard : Keyboard, timer : Timer < hal :: stm32 :: TIM2 > , ui_timer
        : Timer < hal :: stm32 :: TIM3 > , handle : Option <
        __rtic_internal_app_request_MonoTimer_SpawnHandle > , adc : hal :: adc
        :: Adc, photo_r : PhotoR,
    } #[doc = r" Monotonics used by the system"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_Monotonics(pub Systick < 1000 >);
    #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_init_Context <
    'a >
    {
        #[doc = r" Core (Cortex-M) peripherals"] pub core : rtic :: export ::
        Peripherals, #[doc = r" Device peripherals"] pub device : hal :: stm32
        :: Peripherals, #[doc = r" Critical section token for init"] pub cs :
        rtic :: export :: CriticalSection < 'a > ,
    } impl < 'a > __rtic_internal_init_Context < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(core : rtic :: export :: Peripherals,) -> Self
        {
            __rtic_internal_init_Context
            {
                device : hal :: stm32 :: Peripherals :: steal(), cs : rtic ::
                export :: CriticalSection :: new(), core,
            }
        }
    } #[allow(non_snake_case)] #[doc = " Initialization function"] pub mod
    init
    {
        #[doc(inline)] pub use super :: __rtic_internal_Monotonics as
        Monotonics; #[doc(inline)] pub use super ::
        __rtic_internal_init_Context as Context;
    } mod shared_resources
    {
        use rtic :: export :: Priority; #[doc(hidden)]
        #[allow(non_camel_case_types)] pub struct
        power_that_needs_to_be_locked < 'a > { priority : & 'a Priority, }
        impl < 'a > power_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { power_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        rtc_that_needs_to_be_locked < 'a > { priority : & 'a Priority, } impl
        < 'a > rtc_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { rtc_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        lcd_that_needs_to_be_locked < 'a > { priority : & 'a Priority, } impl
        < 'a > lcd_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { lcd_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        hour_history_that_needs_to_be_locked < 'a >
        { priority : & 'a Priority, } impl < 'a >
        hour_history_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { hour_history_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        day_history_that_needs_to_be_locked < 'a >
        { priority : & 'a Priority, } impl < 'a >
        day_history_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { day_history_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        month_history_that_needs_to_be_locked < 'a >
        { priority : & 'a Priority, } impl < 'a >
        month_history_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { month_history_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        storage_that_needs_to_be_locked < 'a > { priority : & 'a Priority, }
        impl < 'a > storage_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { storage_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        app_that_needs_to_be_locked < 'a > { priority : & 'a Priority, } impl
        < 'a > app_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { app_that_needs_to_be_locked { priority } }
            #[inline(always)] pub unsafe fn priority(& self) -> & Priority
            { self.priority }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        ui_that_needs_to_be_locked < 'a > { priority : & 'a Priority, } impl <
        'a > ui_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new(priority : & 'a Priority) ->
            Self { ui_that_needs_to_be_locked { priority } } #[inline(always)]
            pub unsafe fn priority(& self) -> & Priority { self.priority }
        }
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Shared resources `rtc_timer` has access to"] pub struct
    __rtic_internal_rtc_timerSharedResources < 'a >
    {
        #[doc =
        " Resource proxy resource `power`. Use method `.lock()` to gain access"]
        pub power : shared_resources :: power_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `rtc`. Use method `.lock()` to gain access"]
        pub rtc : shared_resources :: rtc_that_needs_to_be_locked < 'a > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_rtc_timer_Context < 'a >
    {
        #[doc = r" Shared Resources this task has access to"] pub shared :
        rtc_timer :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_rtc_timer_Context < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_rtc_timer_Context
            { shared : rtc_timer :: SharedResources :: new(priority), }
        }
    } #[allow(non_snake_case)] #[doc = " Hardware task"] pub mod rtc_timer
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_rtc_timerSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_rtc_timer_Context as
        Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Shared resources `exti9_5` has access to"] pub struct
    __rtic_internal_exti9_5SharedResources < 'a >
    {
        #[doc =
        " Resource proxy resource `power`. Use method `.lock()` to gain access"]
        pub power : shared_resources :: power_that_needs_to_be_locked < 'a > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_exti9_5_Context
    < 'a >
    {
        #[doc = r" Shared Resources this task has access to"] pub shared :
        exti9_5 :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_exti9_5_Context < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_exti9_5_Context
            { shared : exti9_5 :: SharedResources :: new(priority), }
        }
    } #[allow(non_snake_case)] #[doc = " Hardware task"] pub mod exti9_5
    {
        #[doc(inline)] pub use super :: __rtic_internal_exti9_5SharedResources
        as SharedResources; #[doc(inline)] pub use super ::
        __rtic_internal_exti9_5_Context as Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Local resources `timer` has access to"] pub struct
    __rtic_internal_timerLocalResources < 'a >
    {
        #[doc = " Local resource `handle`"] pub handle : & 'a mut Option <
        __rtic_internal_app_request_MonoTimer_SpawnHandle > ,
        #[doc = " Local resource `keyboard`"] pub keyboard : & 'a mut
        Keyboard, #[doc = " Local resource `timer`"] pub timer : & 'a mut
        Timer < hal :: stm32 :: TIM2 > ,
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Shared resources `timer` has access to"] pub struct
    __rtic_internal_timerSharedResources < 'a >
    {
        #[doc =
        " Resource proxy resource `power`. Use method `.lock()` to gain access"]
        pub power : shared_resources :: power_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `lcd`. Use method `.lock()` to gain access"]
        pub lcd : shared_resources :: lcd_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `app`. Use method `.lock()` to gain access"]
        pub app : shared_resources :: app_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `ui`. Use method `.lock()` to gain access"]
        pub ui : shared_resources :: ui_that_needs_to_be_locked < 'a > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_timer_Context <
    'a >
    {
        #[doc = r" Local Resources this task has access to"] pub local : timer
        :: LocalResources < 'a > ,
        #[doc = r" Shared Resources this task has access to"] pub shared :
        timer :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_timer_Context < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_timer_Context
            {
                local : timer :: LocalResources :: new(), shared : timer ::
                SharedResources :: new(priority),
            }
        }
    } #[allow(non_snake_case)] #[doc = " Hardware task"] pub mod timer
    {
        #[doc(inline)] pub use super :: __rtic_internal_timerLocalResources as
        LocalResources; #[doc(inline)] pub use super ::
        __rtic_internal_timerSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_timer_Context as
        Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Local resources `ui_timer` has access to"] pub struct
    __rtic_internal_ui_timerLocalResources < 'a >
    {
        #[doc = " Local resource `ui_timer`"] pub ui_timer : & 'a mut Timer <
        hal :: stm32 :: TIM3 > , #[doc = " Local resource `adc`"] pub adc : &
        'a mut hal :: adc :: Adc, #[doc = " Local resource `photo_r`"] pub
        photo_r : & 'a mut PhotoR, #[doc = " Local resource `led`"] pub led :
        & 'a mut bool,
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Shared resources `ui_timer` has access to"] pub struct
    __rtic_internal_ui_timerSharedResources < 'a >
    {
        #[doc =
        " Resource proxy resource `power`. Use method `.lock()` to gain access"]
        pub power : shared_resources :: power_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `lcd`. Use method `.lock()` to gain access"]
        pub lcd : shared_resources :: lcd_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `app`. Use method `.lock()` to gain access"]
        pub app : shared_resources :: app_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `ui`. Use method `.lock()` to gain access"]
        pub ui : shared_resources :: ui_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `rtc`. Use method `.lock()` to gain access"]
        pub rtc : shared_resources :: rtc_that_needs_to_be_locked < 'a > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_ui_timer_Context
    < 'a >
    {
        #[doc = r" Local Resources this task has access to"] pub local :
        ui_timer :: LocalResources < 'a > ,
        #[doc = r" Shared Resources this task has access to"] pub shared :
        ui_timer :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_ui_timer_Context < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_ui_timer_Context
            {
                local : ui_timer :: LocalResources :: new(), shared : ui_timer
                :: SharedResources :: new(priority),
            }
        }
    } #[allow(non_snake_case)] #[doc = " Hardware task"] pub mod ui_timer
    {
        #[doc(inline)] pub use super :: __rtic_internal_ui_timerLocalResources
        as LocalResources; #[doc(inline)] pub use super ::
        __rtic_internal_ui_timerSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_ui_timer_Context as
        Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = " Shared resources `app_request` has access to"] pub struct
    __rtic_internal_app_requestSharedResources < 'a >
    {
        #[doc =
        " Resource proxy resource `power`. Use method `.lock()` to gain access"]
        pub power : shared_resources :: power_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `lcd`. Use method `.lock()` to gain access"]
        pub lcd : shared_resources :: lcd_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `rtc`. Use method `.lock()` to gain access"]
        pub rtc : shared_resources :: rtc_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `app`. Use method `.lock()` to gain access"]
        pub app : shared_resources :: app_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `hour_history`. Use method `.lock()` to gain access"]
        pub hour_history : shared_resources ::
        hour_history_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `day_history`. Use method `.lock()` to gain access"]
        pub day_history : shared_resources ::
        day_history_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `month_history`. Use method `.lock()` to gain access"]
        pub month_history : shared_resources ::
        month_history_that_needs_to_be_locked < 'a > ,
        #[doc =
        " Resource proxy resource `storage`. Use method `.lock()` to gain access"]
        pub storage : shared_resources :: storage_that_needs_to_be_locked < 'a
        > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_app_request_Context < 'a >
    {
        #[doc = r" Shared Resources this task has access to"] pub shared :
        app_request :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_app_request_Context < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_app_request_Context
            { shared : app_request :: SharedResources :: new(priority), }
        }
    } #[doc = r" Spawns the task directly"] pub fn
    __rtic_internal_app_request_spawn(_0 : AppRequest,) -> Result < (),
    AppRequest >
    {
        let input = _0; unsafe
        {
            if let Some(index) = rtic :: export :: interrupt ::
            free(| _ |
            (& mut * __rtic_internal_app_request_FQ.get_mut()).dequeue())
            {
                (& mut *
                __rtic_internal_app_request_INPUTS.get_mut()).get_unchecked_mut(usize
                :: from(index)).as_mut_ptr().write(input); rtic :: export ::
                interrupt ::
                free(| _ |
                {
                    (& mut *
                    __rtic_internal_P1_RQ.get_mut()).enqueue_unchecked((P1_T ::
                    app_request, index));
                }); rtic :: pend(hal :: stm32 :: interrupt :: AES); Ok(())
            } else { Err(input) }
        }
    } #[doc(hidden)] #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    pub struct __rtic_internal_app_request_MonoTimer_SpawnHandle
    { #[doc(hidden)] marker : u32, } impl core :: fmt :: Debug for
    __rtic_internal_app_request_MonoTimer_SpawnHandle
    {
        #[doc(hidden)] fn
        fmt(& self, f : & mut core :: fmt :: Formatter < '_ >) -> core :: fmt
        :: Result { f.debug_struct("MonoTimer::SpawnHandle").finish() }
    } impl __rtic_internal_app_request_MonoTimer_SpawnHandle
    {
        pub fn cancel(self) -> Result < AppRequest, () >
        {
            rtic :: export :: interrupt ::
            free(| _ | unsafe
            {
                let tq = & mut * __rtic_internal_TQ_MonoTimer.get_mut(); if
                let Some((_task, index)) = tq.cancel_marker(self.marker)
                {
                    let msg =
                    (& *
                    __rtic_internal_app_request_INPUTS.get()).get_unchecked(usize
                    :: from(index)).as_ptr().read();
                    (& mut *
                    __rtic_internal_app_request_FQ.get_mut()).split().0.enqueue_unchecked(index);
                    Ok(msg)
                } else { Err(()) }
            })
        } #[doc = r" Reschedule after "] #[inline] pub fn
        reschedule_after(self, duration : < MonoTimer as rtic :: Monotonic >
        :: Duration) -> Result < Self, () >
        { self.reschedule_at(monotonics :: MonoTimer :: now() + duration) }
        #[doc = r" Reschedule at "] pub fn
        reschedule_at(self, instant : < MonoTimer as rtic :: Monotonic > ::
        Instant) -> Result < Self, () >
        {
            rtic :: export :: interrupt ::
            free(| _ | unsafe
            {
                let marker = __rtic_internal_TIMER_QUEUE_MARKER.get().read();
                __rtic_internal_TIMER_QUEUE_MARKER.get_mut().write(marker.wrapping_add(1));
                let tq = (& mut * __rtic_internal_TQ_MonoTimer.get_mut());
                tq.update_marker(self.marker, marker, instant, || rtic ::
                export :: SCB ::
                set_pendst()).map(| _ | app_request :: MonoTimer ::
                SpawnHandle { marker })
            })
        }
    }
    #[doc =
    r" Spawns the task after a set duration relative to the current time"]
    #[doc = r""]
    #[doc =
    r" This will use the time `Instant::new(0)` as baseline if called in `#[init]`,"]
    #[doc =
    r" so if you use a non-resetable timer use `spawn_at` when in `#[init]`"]
    #[allow(non_snake_case)] pub fn
    __rtic_internal_app_request_MonoTimer_spawn_after(duration : < MonoTimer
    as rtic :: Monotonic > :: Duration, _0 : AppRequest) -> Result <
    app_request :: MonoTimer :: SpawnHandle, AppRequest >
    {
        let instant = monotonics :: MonoTimer :: now();
        __rtic_internal_app_request_MonoTimer_spawn_at(instant + duration, _0)
    } #[doc = r" Spawns the task at a fixed time instant"]
    #[allow(non_snake_case)] pub fn
    __rtic_internal_app_request_MonoTimer_spawn_at(instant : < MonoTimer as
    rtic :: Monotonic > :: Instant, _0 : AppRequest) -> Result < app_request
    :: MonoTimer :: SpawnHandle, AppRequest >
    {
        unsafe
        {
            let input = _0; if let Some(index) = rtic :: export :: interrupt
            ::
            free(| _ |
            (& mut * __rtic_internal_app_request_FQ.get_mut()).dequeue())
            {
                (& mut *
                __rtic_internal_app_request_INPUTS.get_mut()).get_unchecked_mut(usize
                :: from(index)).as_mut_ptr().write(input);
                (& mut *
                __rtic_internal_app_request_MonoTimer_INSTANTS.get_mut()).get_unchecked_mut(usize
                :: from(index)).as_mut_ptr().write(instant); rtic :: export ::
                interrupt ::
                free(| _ |
                {
                    let marker =
                    __rtic_internal_TIMER_QUEUE_MARKER.get().read(); let nr =
                    rtic :: export :: NotReady
                    { instant, index, task : SCHED_T :: app_request, marker, };
                    __rtic_internal_TIMER_QUEUE_MARKER.get_mut().write(__rtic_internal_TIMER_QUEUE_MARKER.get().read().wrapping_add(1));
                    let tq = & mut * __rtic_internal_TQ_MonoTimer.get_mut();
                    tq.enqueue_unchecked(nr, || core :: mem :: transmute :: < _,
                    rtic :: export :: SYST > (()).enable_interrupt(), || rtic ::
                    export :: SCB :: set_pendst(),
                    (& mut *
                    __rtic_internal_MONOTONIC_STORAGE_MonoTimer.get_mut()).as_mut());
                    Ok(app_request :: MonoTimer :: SpawnHandle { marker })
                })
            } else { Err(input) }
        }
    } #[allow(non_snake_case)] #[doc = " Software task"] pub mod app_request
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_app_requestSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_app_request_Context as
        Context; #[doc(inline)] pub use super ::
        __rtic_internal_app_request_spawn as spawn; pub use MonoTimer ::
        spawn_after; pub use MonoTimer :: spawn_at; pub use MonoTimer ::
        SpawnHandle; #[doc(hidden)] pub mod MonoTimer
        {
            pub use super :: super ::
            __rtic_internal_app_request_MonoTimer_spawn_after as spawn_after;
            pub use super :: super ::
            __rtic_internal_app_request_MonoTimer_spawn_at as spawn_at; pub
            use super :: super ::
            __rtic_internal_app_request_MonoTimer_SpawnHandle as SpawnHandle;
        }
    } #[doc = r" App module"] #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] #[doc(hidden)]
    #[link_section = ".uninit.rtic0"] static
    __rtic_internal_shared_resource_power : rtic :: RacyCell < core :: mem ::
    MaybeUninit < hardware :: Power >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: power_that_needs_to_be_locked < 'a >
    {
        type T = hardware :: Power; #[inline(always)] fn lock <
        RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut hardware :: Power) ->
        RTIC_INTERNAL_R) -> RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_power.get_mut() as * mut
                _, self.priority(), CEILING, hal :: stm32 :: NVIC_PRIO_BITS, &
                __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic1"] static
    __rtic_internal_shared_resource_rtc : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Rtc < hal :: rtc :: Lse > >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: rtc_that_needs_to_be_locked < 'a >
    {
        type T = Rtc < hal :: rtc :: Lse > ; #[inline(always)] fn lock <
        RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut Rtc < hal :: rtc :: Lse >) ->
        RTIC_INTERNAL_R) -> RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_rtc.get_mut() as * mut _,
                self.priority(), CEILING, hal :: stm32 :: NVIC_PRIO_BITS, &
                __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic2"] static
    __rtic_internal_shared_resource_lcd : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Lcd >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: lcd_that_needs_to_be_locked < 'a >
    {
        type T = Lcd; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut Lcd) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_lcd.get_mut() as * mut _,
                self.priority(), CEILING, hal :: stm32 :: NVIC_PRIO_BITS, &
                __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic3"] static
    __rtic_internal_shared_resource_hour_history : rtic :: RacyCell < core ::
    mem :: MaybeUninit < HourHistory >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: hour_history_that_needs_to_be_locked < 'a >
    {
        type T = HourHistory; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut HourHistory) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 1u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_hour_history.get_mut() as
                * mut _, self.priority(), CEILING, hal :: stm32 ::
                NVIC_PRIO_BITS, & __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic4"] static
    __rtic_internal_shared_resource_day_history : rtic :: RacyCell < core ::
    mem :: MaybeUninit < DayHistory >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: day_history_that_needs_to_be_locked < 'a >
    {
        type T = DayHistory; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut DayHistory) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 1u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_day_history.get_mut() as
                * mut _, self.priority(), CEILING, hal :: stm32 ::
                NVIC_PRIO_BITS, & __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic5"] static
    __rtic_internal_shared_resource_month_history : rtic :: RacyCell < core ::
    mem :: MaybeUninit < MonthHistory >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: month_history_that_needs_to_be_locked < 'a >
    {
        type T = MonthHistory; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut MonthHistory) -> RTIC_INTERNAL_R)
        -> RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 1u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_month_history.get_mut()
                as * mut _, self.priority(), CEILING, hal :: stm32 ::
                NVIC_PRIO_BITS, & __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic6"] static
    __rtic_internal_shared_resource_storage : rtic :: RacyCell < core :: mem
    :: MaybeUninit < MyStorage >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: storage_that_needs_to_be_locked < 'a >
    {
        type T = MyStorage; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut MyStorage) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 1u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_storage.get_mut() as *
                mut _, self.priority(), CEILING, hal :: stm32 ::
                NVIC_PRIO_BITS, & __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic7"] static
    __rtic_internal_shared_resource_app : rtic :: RacyCell < core :: mem ::
    MaybeUninit < App >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: app_that_needs_to_be_locked < 'a >
    {
        type T = App; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut App) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_app.get_mut() as * mut _,
                self.priority(), CEILING, hal :: stm32 :: NVIC_PRIO_BITS, &
                __rtic_internal_MASKS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic8"] static
    __rtic_internal_shared_resource_ui : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Viewport >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: ui_that_needs_to_be_locked < 'a >
    {
        type T = Viewport; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut Viewport) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_ui.get_mut() as * mut _,
                self.priority(), CEILING, hal :: stm32 :: NVIC_PRIO_BITS, &
                __rtic_internal_MASKS, f,)
            }
        }
    } #[doc(hidden)] #[allow(non_upper_case_globals)] const
    __rtic_internal_MASK_CHUNKS : usize = rtic :: export ::
    compute_mask_chunks([hal :: stm32 :: Interrupt :: AES as u32, hal :: stm32
    :: Interrupt :: RTC_WKUP as u32, hal :: stm32 :: Interrupt :: EXTI9_5 as
    u32, hal :: stm32 :: Interrupt :: TIM2 as u32, hal :: stm32 :: Interrupt
    :: TIM3 as u32]); #[doc(hidden)] #[allow(non_upper_case_globals)] const
    __rtic_internal_MASKS :
    [rtic :: export :: Mask < __rtic_internal_MASK_CHUNKS > ; 3] =
    [rtic :: export :: create_mask([hal :: stm32 :: Interrupt :: AES as u32]),
    rtic :: export ::
    create_mask([hal :: stm32 :: Interrupt :: RTC_WKUP as u32, hal :: stm32 ::
    Interrupt :: EXTI9_5 as u32, hal :: stm32 :: Interrupt :: TIM2 as u32, hal
    :: stm32 :: Interrupt :: TIM3 as u32]), rtic :: export ::
    create_mask([])]; #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] #[doc(hidden)]
    #[link_section = ".uninit.rtic9"] static
    __rtic_internal_local_resource_keyboard : rtic :: RacyCell < core :: mem
    :: MaybeUninit < Keyboard >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic10"] static
    __rtic_internal_local_resource_timer : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Timer < hal :: stm32 :: TIM2 > >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic11"] static
    __rtic_internal_local_resource_ui_timer : rtic :: RacyCell < core :: mem
    :: MaybeUninit < Timer < hal :: stm32 :: TIM3 > >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic12"] static
    __rtic_internal_local_resource_handle : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Option < __rtic_internal_app_request_MonoTimer_SpawnHandle >
    >> = rtic :: RacyCell :: new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic13"] static
    __rtic_internal_local_resource_adc : rtic :: RacyCell < core :: mem ::
    MaybeUninit < hal :: adc :: Adc >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic14"] static
    __rtic_internal_local_resource_photo_r : rtic :: RacyCell < core :: mem ::
    MaybeUninit < PhotoR >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] static __rtic_internal_local_ui_timer_led : rtic ::
    RacyCell < bool > = rtic :: RacyCell :: new(false);
    #[allow(non_snake_case)] #[no_mangle]
    #[doc = " User HW task ISR trampoline for rtc_timer"] unsafe fn RTC_WKUP()
    {
        const PRIORITY : u8 = 2u8; rtic :: export ::
        run(PRIORITY, ||
        {
            rtc_timer(rtc_timer :: Context ::
            new(& rtic :: export :: Priority :: new(PRIORITY)))
        });
    } impl < 'a > __rtic_internal_rtc_timerSharedResources < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_rtc_timerSharedResources
            {
                #[doc(hidden)] power : shared_resources ::
                power_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                rtc : shared_resources :: rtc_that_needs_to_be_locked ::
                new(priority),
            }
        }
    } #[allow(non_snake_case)] #[no_mangle]
    #[doc = " User HW task ISR trampoline for exti9_5"] unsafe fn EXTI9_5()
    {
        const PRIORITY : u8 = 2u8; rtic :: export ::
        run(PRIORITY, ||
        {
            exti9_5(exti9_5 :: Context ::
            new(& rtic :: export :: Priority :: new(PRIORITY)))
        });
    } impl < 'a > __rtic_internal_exti9_5SharedResources < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_exti9_5SharedResources
            {
                #[doc(hidden)] power : shared_resources ::
                power_that_needs_to_be_locked :: new(priority),
            }
        }
    } #[allow(non_snake_case)] #[no_mangle]
    #[doc = " User HW task ISR trampoline for timer"] unsafe fn TIM2()
    {
        const PRIORITY : u8 = 2u8; rtic :: export ::
        run(PRIORITY, ||
        {
            timer(timer :: Context ::
            new(& rtic :: export :: Priority :: new(PRIORITY)))
        });
    } impl < 'a > __rtic_internal_timerLocalResources < 'a >
    {
        #[inline(always)] #[doc(hidden)] pub unsafe fn new() -> Self
        {
            __rtic_internal_timerLocalResources
            {
                handle : & mut *
                (& mut *
                __rtic_internal_local_resource_handle.get_mut()).as_mut_ptr(),
                keyboard : & mut *
                (& mut *
                __rtic_internal_local_resource_keyboard.get_mut()).as_mut_ptr(),
                timer : & mut *
                (& mut *
                __rtic_internal_local_resource_timer.get_mut()).as_mut_ptr(),
            }
        }
    } impl < 'a > __rtic_internal_timerSharedResources < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_timerSharedResources
            {
                #[doc(hidden)] power : shared_resources ::
                power_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                lcd : shared_resources :: lcd_that_needs_to_be_locked ::
                new(priority), #[doc(hidden)] app : shared_resources ::
                app_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                ui : shared_resources :: ui_that_needs_to_be_locked ::
                new(priority),
            }
        }
    } #[allow(non_snake_case)] #[no_mangle]
    #[doc = " User HW task ISR trampoline for ui_timer"] unsafe fn TIM3()
    {
        const PRIORITY : u8 = 2u8; rtic :: export ::
        run(PRIORITY, ||
        {
            ui_timer(ui_timer :: Context ::
            new(& rtic :: export :: Priority :: new(PRIORITY)))
        });
    } impl < 'a > __rtic_internal_ui_timerLocalResources < 'a >
    {
        #[inline(always)] #[doc(hidden)] pub unsafe fn new() -> Self
        {
            __rtic_internal_ui_timerLocalResources
            {
                ui_timer : & mut *
                (& mut *
                __rtic_internal_local_resource_ui_timer.get_mut()).as_mut_ptr(),
                adc : & mut *
                (& mut *
                __rtic_internal_local_resource_adc.get_mut()).as_mut_ptr(),
                photo_r : & mut *
                (& mut *
                __rtic_internal_local_resource_photo_r.get_mut()).as_mut_ptr(),
                led : & mut * __rtic_internal_local_ui_timer_led.get_mut(),
            }
        }
    } impl < 'a > __rtic_internal_ui_timerSharedResources < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_ui_timerSharedResources
            {
                #[doc(hidden)] power : shared_resources ::
                power_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                lcd : shared_resources :: lcd_that_needs_to_be_locked ::
                new(priority), #[doc(hidden)] app : shared_resources ::
                app_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                ui : shared_resources :: ui_that_needs_to_be_locked ::
                new(priority), #[doc(hidden)] rtc : shared_resources ::
                rtc_that_needs_to_be_locked :: new(priority),
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] static __rtic_internal_app_request_FQ : rtic :: RacyCell <
    rtic :: export :: SCFQ < 9 > > = rtic :: RacyCell ::
    new(rtic :: export :: Queue :: new()); #[link_section = ".uninit.rtic15"]
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] static __rtic_internal_app_request_MonoTimer_INSTANTS :
    rtic :: RacyCell <
    [core :: mem :: MaybeUninit << Systick < 1000 > as rtic :: Monotonic > ::
    Instant > ; 8] > = rtic :: RacyCell ::
    new([core :: mem :: MaybeUninit :: uninit(), core :: mem :: MaybeUninit ::
    uninit(), core :: mem :: MaybeUninit :: uninit(), core :: mem ::
    MaybeUninit :: uninit(), core :: mem :: MaybeUninit :: uninit(), core ::
    mem :: MaybeUninit :: uninit(), core :: mem :: MaybeUninit :: uninit(),
    core :: mem :: MaybeUninit :: uninit(),]);
    #[link_section = ".uninit.rtic16"] #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] #[doc(hidden)] static
    __rtic_internal_app_request_INPUTS : rtic :: RacyCell <
    [core :: mem :: MaybeUninit < AppRequest > ; 8] > = rtic :: RacyCell ::
    new([core :: mem :: MaybeUninit :: uninit(), core :: mem :: MaybeUninit ::
    uninit(), core :: mem :: MaybeUninit :: uninit(), core :: mem ::
    MaybeUninit :: uninit(), core :: mem :: MaybeUninit :: uninit(), core ::
    mem :: MaybeUninit :: uninit(), core :: mem :: MaybeUninit :: uninit(),
    core :: mem :: MaybeUninit :: uninit(),]); impl < 'a >
    __rtic_internal_app_requestSharedResources < 'a >
    {
        #[doc(hidden)] #[inline(always)] pub unsafe fn
        new(priority : & 'a rtic :: export :: Priority) -> Self
        {
            __rtic_internal_app_requestSharedResources
            {
                #[doc(hidden)] power : shared_resources ::
                power_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                lcd : shared_resources :: lcd_that_needs_to_be_locked ::
                new(priority), #[doc(hidden)] rtc : shared_resources ::
                rtc_that_needs_to_be_locked :: new(priority), #[doc(hidden)]
                app : shared_resources :: app_that_needs_to_be_locked ::
                new(priority), #[doc(hidden)] hour_history : shared_resources
                :: hour_history_that_needs_to_be_locked :: new(priority),
                #[doc(hidden)] day_history : shared_resources ::
                day_history_that_needs_to_be_locked :: new(priority),
                #[doc(hidden)] month_history : shared_resources ::
                month_history_that_needs_to_be_locked :: new(priority),
                #[doc(hidden)] storage : shared_resources ::
                storage_that_needs_to_be_locked :: new(priority),
            }
        }
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[derive(Clone, Copy)] #[doc(hidden)] pub enum P1_T { app_request, }
    #[doc(hidden)] #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] static __rtic_internal_P1_RQ : rtic ::
    RacyCell < rtic :: export :: SCRQ < P1_T, 9 > > = rtic :: RacyCell ::
    new(rtic :: export :: Queue :: new()); #[allow(non_snake_case)]
    #[doc = "Interrupt handler to dispatch tasks at priority 1"] #[no_mangle]
    unsafe fn AES()
    {
        #[doc = r" The priority of this interrupt handler"] const PRIORITY :
        u8 = 1u8; rtic :: export ::
        run(PRIORITY, ||
        {
            while let Some((task, index)) =
            (& mut * __rtic_internal_P1_RQ.get_mut()).split().1.dequeue()
            {
                match task
                {
                    P1_T :: app_request =>
                    {
                        let _0 =
                        (& *
                        __rtic_internal_app_request_INPUTS.get()).get_unchecked(usize
                        :: from(index)).as_ptr().read();
                        (& mut *
                        __rtic_internal_app_request_FQ.get_mut()).split().0.enqueue_unchecked(index);
                        let priority = & rtic :: export :: Priority ::
                        new(PRIORITY);
                        app_request(app_request :: Context :: new(priority), _0)
                    }
                }
            }
        });
    } #[doc(hidden)] #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] static __rtic_internal_TIMER_QUEUE_MARKER
    : rtic :: RacyCell < u32 > = rtic :: RacyCell :: new(0); #[doc(hidden)]
    #[allow(non_camel_case_types)] #[derive(Clone, Copy)] pub enum SCHED_T
    { app_request, } #[doc(hidden)] #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] static __rtic_internal_TQ_MonoTimer :
    rtic :: RacyCell < rtic :: export :: TimerQueue < Systick < 1000 > ,
    SCHED_T, 8 > > = rtic :: RacyCell ::
    new(rtic :: export ::
    TimerQueue(rtic :: export :: SortedLinkedList :: new_u16()));
    #[doc(hidden)] #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] static
    __rtic_internal_MONOTONIC_STORAGE_MonoTimer : rtic :: RacyCell < Option <
    Systick < 1000 > >> = rtic :: RacyCell :: new(None); #[no_mangle]
    #[allow(non_snake_case)] unsafe fn SysTick()
    {
        while let Some((task, index)) = rtic :: export :: interrupt ::
        free(| _ | if let Some(mono) =
        (& mut *
        __rtic_internal_MONOTONIC_STORAGE_MonoTimer.get_mut()).as_mut()
        {
            (& mut *
            __rtic_internal_TQ_MonoTimer.get_mut()).dequeue(|| core :: mem ::
            transmute :: < _, rtic :: export :: SYST >
            (()).disable_interrupt(), mono)
        } else { core :: hint :: unreachable_unchecked() })
        {
            match task
            {
                SCHED_T :: app_request =>
                {
                    rtic :: export :: interrupt ::
                    free(| _ |
                    (& mut *
                    __rtic_internal_P1_RQ.get_mut()).split().0.enqueue_unchecked((P1_T
                    :: app_request, index))); rtic ::
                    pend(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
                    :: interrupt :: AES);
                }
            }
        } rtic :: export :: interrupt ::
        free(| _ | if let Some(mono) =
        (& mut *
        __rtic_internal_MONOTONIC_STORAGE_MonoTimer.get_mut()).as_mut()
        { mono.on_interrupt(); });
    } #[doc(hidden)] mod rtic_ext
    {
        use super :: * ; #[no_mangle] unsafe extern "C" fn main() -> !
        {
            rtic :: export :: assert_send :: < hardware :: Power > (); rtic ::
            export :: assert_send :: < Rtc < hal :: rtc :: Lse > > (); rtic ::
            export :: assert_send :: < Lcd > (); rtic :: export :: assert_send
            :: < HourHistory > (); rtic :: export :: assert_send :: <
            DayHistory > (); rtic :: export :: assert_send :: < MonthHistory >
            (); rtic :: export :: assert_send :: < MyStorage > (); rtic ::
            export :: assert_send :: < App > (); rtic :: export :: assert_send
            :: < Viewport > (); rtic :: export :: assert_send :: < Keyboard >
            (); rtic :: export :: assert_send :: < Timer < hal :: stm32 ::
            TIM2 > > (); rtic :: export :: assert_send :: < Timer < hal ::
            stm32 :: TIM3 > > (); rtic :: export :: assert_send :: < Option <
            __rtic_internal_app_request_MonoTimer_SpawnHandle > > (); rtic ::
            export :: assert_send :: < hal :: adc :: Adc > (); rtic :: export
            :: assert_send :: < PhotoR > (); rtic :: export :: assert_send ::
            < AppRequest > (); rtic :: export :: assert_monotonic :: < Systick
            < 1000 > > (); const _CONST_CHECK : () =
            {
                if ! rtic :: export :: have_basepri()
                {
                    if (hal :: stm32 :: Interrupt :: RTC_WKUP as usize) >=
                    (__rtic_internal_MASK_CHUNKS * 32)
                    {
                        :: core :: panic!
                        ("An interrupt out of range is used while in armv6 or armv8m.base");
                    } if (hal :: stm32 :: Interrupt :: EXTI9_5 as usize) >=
                    (__rtic_internal_MASK_CHUNKS * 32)
                    {
                        :: core :: panic!
                        ("An interrupt out of range is used while in armv6 or armv8m.base");
                    } if (hal :: stm32 :: Interrupt :: TIM2 as usize) >=
                    (__rtic_internal_MASK_CHUNKS * 32)
                    {
                        :: core :: panic!
                        ("An interrupt out of range is used while in armv6 or armv8m.base");
                    } if (hal :: stm32 :: Interrupt :: TIM3 as usize) >=
                    (__rtic_internal_MASK_CHUNKS * 32)
                    {
                        :: core :: panic!
                        ("An interrupt out of range is used while in armv6 or armv8m.base");
                    }
                } else {}
            }; let _ = _CONST_CHECK; rtic :: export :: interrupt :: disable();
            (0 ..
            8u8).for_each(| i |
            (& mut *
            __rtic_internal_app_request_FQ.get_mut()).enqueue_unchecked(i));
            let mut core : rtic :: export :: Peripherals = rtic :: export ::
            Peripherals :: steal().into(); let _ =
            you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml ::
            interrupt :: AES; let _ =
            you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml ::
            interrupt :: COMP_ACQ; const _ : () = if
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) < 1u8 as usize
            {
                :: core :: panic!
                ("Maximum priority used by interrupt vector 'AES' is more than supported by hardware");
            };
            core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: AES, rtic :: export ::
            logical2hw(1u8, hal :: stm32 :: NVIC_PRIO_BITS),); rtic :: export
            :: NVIC ::
            unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: AES); const _ : () = if
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) < 2u8 as usize
            {
                :: core :: panic!
                ("Maximum priority used by interrupt vector 'RTC_WKUP' is more than supported by hardware");
            };
            core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: RTC_WKUP, rtic :: export ::
            logical2hw(2u8, hal :: stm32 :: NVIC_PRIO_BITS),); rtic :: export
            :: NVIC ::
            unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: RTC_WKUP); const _ : () = if
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) < 2u8 as usize
            {
                :: core :: panic!
                ("Maximum priority used by interrupt vector 'EXTI9_5' is more than supported by hardware");
            };
            core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: EXTI9_5, rtic :: export ::
            logical2hw(2u8, hal :: stm32 :: NVIC_PRIO_BITS),); rtic :: export
            :: NVIC ::
            unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: EXTI9_5); const _ : () = if
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) < 2u8 as usize
            {
                :: core :: panic!
                ("Maximum priority used by interrupt vector 'TIM2' is more than supported by hardware");
            };
            core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: TIM2, rtic :: export ::
            logical2hw(2u8, hal :: stm32 :: NVIC_PRIO_BITS),); rtic :: export
            :: NVIC ::
            unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: TIM2); const _ : () = if
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) < 2u8 as usize
            {
                :: core :: panic!
                ("Maximum priority used by interrupt vector 'TIM3' is more than supported by hardware");
            };
            core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: TIM3, rtic :: export ::
            logical2hw(2u8, hal :: stm32 :: NVIC_PRIO_BITS),); rtic :: export
            :: NVIC ::
            unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
            :: interrupt :: TIM3); const _ : () = if
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) <
            (1 << hal :: stm32 :: NVIC_PRIO_BITS) as usize
            {
                :: core :: panic!
                ("Maximum priority used by monotonic 'MonoTimer' is more than supported by hardware");
            };
            core.SCB.set_priority(rtic :: export :: SystemHandler :: SysTick,
            rtic :: export ::
            logical2hw((1 << hal :: stm32 :: NVIC_PRIO_BITS), hal :: stm32 ::
            NVIC_PRIO_BITS),); if ! < Systick < 1000 > as rtic :: Monotonic >
            :: DISABLE_INTERRUPT_ON_EMPTY_QUEUE
            {
                core :: mem :: transmute :: < _, rtic :: export :: SYST >
                (()).enable_interrupt();
            } #[inline(never)] fn __rtic_init_resources < F > (f : F) where F
            : FnOnce() { f(); }
            __rtic_init_resources(||
            {
                let (shared_resources, local_resources, mut monotonics) =
                init(init :: Context :: new(core.into()));
                __rtic_internal_shared_resource_power.get_mut().write(core ::
                mem :: MaybeUninit :: new(shared_resources.power));
                __rtic_internal_shared_resource_rtc.get_mut().write(core ::
                mem :: MaybeUninit :: new(shared_resources.rtc));
                __rtic_internal_shared_resource_lcd.get_mut().write(core ::
                mem :: MaybeUninit :: new(shared_resources.lcd));
                __rtic_internal_shared_resource_hour_history.get_mut().write(core
                :: mem :: MaybeUninit :: new(shared_resources.hour_history));
                __rtic_internal_shared_resource_day_history.get_mut().write(core
                :: mem :: MaybeUninit :: new(shared_resources.day_history));
                __rtic_internal_shared_resource_month_history.get_mut().write(core
                :: mem :: MaybeUninit :: new(shared_resources.month_history));
                __rtic_internal_shared_resource_storage.get_mut().write(core
                :: mem :: MaybeUninit :: new(shared_resources.storage));
                __rtic_internal_shared_resource_app.get_mut().write(core ::
                mem :: MaybeUninit :: new(shared_resources.app));
                __rtic_internal_shared_resource_ui.get_mut().write(core :: mem
                :: MaybeUninit :: new(shared_resources.ui));
                __rtic_internal_local_resource_keyboard.get_mut().write(core
                :: mem :: MaybeUninit :: new(local_resources.keyboard));
                __rtic_internal_local_resource_timer.get_mut().write(core ::
                mem :: MaybeUninit :: new(local_resources.timer));
                __rtic_internal_local_resource_ui_timer.get_mut().write(core
                :: mem :: MaybeUninit :: new(local_resources.ui_timer));
                __rtic_internal_local_resource_handle.get_mut().write(core ::
                mem :: MaybeUninit :: new(local_resources.handle));
                __rtic_internal_local_resource_adc.get_mut().write(core :: mem
                :: MaybeUninit :: new(local_resources.adc));
                __rtic_internal_local_resource_photo_r.get_mut().write(core ::
                mem :: MaybeUninit :: new(local_resources.photo_r));
                monotonics.0.reset();
                __rtic_internal_MONOTONIC_STORAGE_MonoTimer.get_mut().write(Some(monotonics.0));
                rtic :: export :: interrupt :: enable();
            }); loop { rtic :: export :: nop() }
        }
    }
}