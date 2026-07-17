// 阻止 release 模式弹出额外控制台窗口，勿删
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    disk_manager_lib::run()
}