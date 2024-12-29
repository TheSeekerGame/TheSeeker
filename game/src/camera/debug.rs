pub fn debug_print(&self) {
    println!("CameraRig Debug:");
    println!("  Target: {}", self.target);
    println!("  Camera Position: {}", self.camera_position);
    println!("  Displacement: {}", self.displacement);
    println!("  Equilibrium_y: {}", self.equilibrium_y);
}