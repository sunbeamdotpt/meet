import { auth } from '@/auth';
import { getEgressClient } from '@/lib/livekit';
import { NextResponse } from 'next/server';
import {
  EncodedFileOutput,
  EncodedFileType,
  RoomCompositeOptions,
  S3Upload,
} from 'livekit-server-sdk';

interface RouteParams {
  params: Promise<{ roomName: string }>;
}

export async function POST(_request: Request, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;

    const s3KeyId = process.env.S3_KEY_ID;
    const s3KeySecret = process.env.S3_KEY_SECRET;
    const s3Endpoint = process.env.S3_ENDPOINT;
    const s3Bucket = process.env.S3_BUCKET;
    const s3Region = process.env.S3_REGION;

    if (!s3Bucket) {
      return new NextResponse('Egress storage is not configured', { status: 503 });
    }

    const fileOutput = new EncodedFileOutput({
      fileType: EncodedFileType.MP4,
      filepath: `recordings/${roomName}/${Date.now()}.mp4`,
      output: {
        case: 's3',
        value: new S3Upload({
          accessKey: s3KeyId,
          secret: s3KeySecret,
          endpoint: s3Endpoint,
          bucket: s3Bucket,
          region: s3Region,
          forcePathStyle: !!s3Endpoint,
        }),
      },
    });

    const options: RoomCompositeOptions = {
      layout: 'grid',
    };

    const egressClient = getEgressClient();
    const info = await egressClient.startRoomCompositeEgress(roomName, fileOutput, options);

    return NextResponse.json({ egressId: info.egressId, status: info.status });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
