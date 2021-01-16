target remote :1337
set print asm-demangle on
set print pretty on

# detect unhandled exceptions, hard faults and panics
break DefaultHandler
break HardFault
break rust_begin_unwind
