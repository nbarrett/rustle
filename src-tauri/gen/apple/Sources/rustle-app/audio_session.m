#import <AVFoundation/AVFoundation.h>

void rustle_request_microphone_permission(void *context, void (*completion)(void *, bool)) {
    dispatch_async(dispatch_get_main_queue(), ^{
        AVAudioSession *session = [AVAudioSession sharedInstance];
        [session requestRecordPermission:^(BOOL granted) {
            if (!granted) {
                completion(context, false);
                return;
            }
            NSError *error = nil;
            BOOL prepared = [session setCategory:AVAudioSessionCategoryPlayAndRecord
                                            mode:AVAudioSessionModeMeasurement
                                         options:AVAudioSessionCategoryOptionDefaultToSpeaker |
                                                 AVAudioSessionCategoryOptionAllowBluetooth
                                           error:&error];
            prepared = prepared && [session setActive:YES error:&error];
            completion(context, prepared);
        }];
    });
}
