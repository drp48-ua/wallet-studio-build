#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>

@interface WalletStudioOwnMetalControl : UIView
@end
@implementation WalletStudioOwnMetalControl
+ (Class)layerClass { return CAMetalLayer.class; }
@end

// Public Metal API, own red drawable only. No private renderer state is changed.
static NSMutableDictionary *StartOwnGPUCaptureControl(UIView *host) {
    NSMutableDictionary *result=[@{@"setup":@NO,@"command_completed":@NO,
        @"presentation_observed":@NO,@"readback_red":@NO,@"captured_red":@NO,
        @"readback_rgb":@[],@"capture_rgb":@[]} mutableCopy];
    id<MTLDevice> device=MTLCreateSystemDefaultDevice();
    id<MTLCommandQueue> queue=[device newCommandQueue];
    if(!device || !queue) return result;
    WalletStudioOwnMetalControl *control=[[WalletStudioOwnMetalControl alloc] initWithFrame:CGRectMake(16,50,20,20)];
    CAMetalLayer *layer=(CAMetalLayer*)control.layer;
    layer.device=device;layer.pixelFormat=MTLPixelFormatBGRA8Unorm;
    layer.framebufferOnly=NO;layer.drawableSize=CGSizeMake(20,20);layer.opaque=YES;
    [host addSubview:control];[control layoutIfNeeded];
    dispatch_async(dispatch_get_main_queue(), ^{
        @try {
            id<CAMetalDrawable> drawable=[layer nextDrawable];
            id<MTLBuffer> buffer=[device newBufferWithLength:256 options:MTLResourceStorageModeShared];
            id<MTLCommandBuffer> command=[queue commandBuffer];
            if(!drawable || !buffer || !command) { [control removeFromSuperview];return; }
            MTLRenderPassDescriptor *pass=[MTLRenderPassDescriptor renderPassDescriptor];
            pass.colorAttachments[0].texture=drawable.texture;
            pass.colorAttachments[0].loadAction=MTLLoadActionClear;
            pass.colorAttachments[0].storeAction=MTLStoreActionStore;
            pass.colorAttachments[0].clearColor=MTLClearColorMake(1,0,0,1);
            id<MTLRenderCommandEncoder> render=[command renderCommandEncoderWithDescriptor:pass];
            if(!render) { [control removeFromSuperview];return; }
            [render endEncoding];
            id<MTLBlitCommandEncoder> blit=[command blitCommandEncoder];
            if(!blit) { [control removeFromSuperview];return; }
            [blit copyFromTexture:drawable.texture sourceSlice:0 sourceLevel:0
                sourceOrigin:MTLOriginMake(10,10,0) sourceSize:MTLSizeMake(1,1,1)
                toBuffer:buffer destinationOffset:0 destinationBytesPerRow:256 destinationBytesPerImage:256];
            [blit endEncoding];
            [command addCompletedHandler:^(id<MTLCommandBuffer> completed){
                if(completed.status!=MTLCommandBufferStatusCompleted) return;
                const unsigned char *pixel=buffer.contents;
                NSArray *rgb=@[@(pixel[2]),@(pixel[1]),@(pixel[0])];
                BOOL red=pixel[2]>220 && pixel[1]<30 && pixel[0]<30 && pixel[3]==255;
                dispatch_async(dispatch_get_main_queue(), ^{
                    result[@"command_completed"]=@YES;result[@"readback_rgb"]=rgb;result[@"readback_red"]=@(red);
                });
            }];
            [drawable addPresentedHandler:^(id<MTLDrawable> presented){
                dispatch_async(dispatch_get_main_queue(), ^{
                    result[@"presentation_observed"]=@YES;
                    unsigned char alpha=0;NSArray *rgb=ReadSyntheticCenter(control,&alpha);
                    result[@"capture_rgb"]=rgb;
                    BOOL red=alpha==255 && rgb.count==3 && [rgb[0] intValue]>220 && [rgb[1] intValue]<30 && [rgb[2] intValue]<30;
                    result[@"captured_red"]=@(red);
                    [control removeFromSuperview];
                });
            }];
            result[@"setup"]=@YES;
            [command presentDrawable:drawable];[command commit];
        } @catch(NSException *exception) { [control removeFromSuperview]; }
    });
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,3*NSEC_PER_SEC),dispatch_get_main_queue(),^{[control removeFromSuperview];});
    return result;
}
