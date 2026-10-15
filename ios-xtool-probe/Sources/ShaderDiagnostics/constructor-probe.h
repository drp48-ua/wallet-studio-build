// Inspect only the loaded constructor implementation in this app's own process.
// No other task, user data, Wallet process or executable modification is involved.
#import <mach/mach.h>
#import <ptrauth.h>

static NSDictionary *ProbeOwnConstructor(void) {
    NSMutableDictionary *result=[@{@"status":@"unknown",@"reason":@"method_unavailable",
        @"image_origin_verified":@NO,@"code_hex":@""} mutableCopy];
    Class cls=NSClassFromString(@"PKLiveRenderedCardFaceView");
    Method method=class_getInstanceMethod(cls,@selector(initWithFrame:pass:));
    if(!method || method_getNumberOfArguments(method)!=4) return result;
    char ret[128]={0}, frame[128]={0}, pass[128]={0};
    method_getReturnType(method,ret,sizeof(ret));
    method_getArgumentType(method,2,frame,sizeof(frame));
    method_getArgumentType(method,3,pass,sizeof(pass));
    if(ret[0]!='@' || strcmp(frame,@encode(CGRect)) || pass[0]!='@') return result;
    const void *code=(const void*)ptrauth_strip(method_getImplementation(method),ptrauth_key_function_pointer);
    Dl_info image={0};
    if(!dladdr(code,&image) || !image.dli_fname ||
       strcmp(image.dli_fname,"/System/Library/PrivateFrameworks/PassKitUI.framework/PassKitUI")) {
        result[@"reason"]=@"image_origin_unverified"; return result;
    }
    result[@"image_origin_verified"]=@YES;
    unsigned char bytes[512]={0}; vm_size_t read=0;
    kern_return_t status=vm_read_overwrite(mach_task_self(),(vm_address_t)(uintptr_t)code,
        sizeof(bytes),(vm_address_t)(uintptr_t)bytes,&read);
    if(status!=KERN_SUCCESS || read!=sizeof(bytes)) {
        result[@"reason"]=@"own_code_read_unavailable"; return result;
    }
    NSMutableString *hex=[NSMutableString stringWithCapacity:sizeof(bytes)*2];
    for(NSUInteger i=0;i<sizeof(bytes);++i) [hex appendFormat:@"%02x",bytes[i]];
    result[@"status"]=@"observed"; result[@"reason"]=@"own_constructor_code_observed";
    result[@"code_hex"]=hex;
    return result;
}
