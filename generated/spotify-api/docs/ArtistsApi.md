# \ArtistsApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**check_current_user_follows**](ArtistsApi.md#check_current_user_follows) | **GET** /me/following/contains | Check If User Follows Artists or Users 
[**follow_artists_users**](ArtistsApi.md#follow_artists_users) | **PUT** /me/following | Follow Artists or Users 
[**get_an_artist**](ArtistsApi.md#get_an_artist) | **GET** /artists/{id} | Get Artist 
[**get_an_artists_albums**](ArtistsApi.md#get_an_artists_albums) | **GET** /artists/{id}/albums | Get Artist's Albums 
[**get_an_artists_related_artists**](ArtistsApi.md#get_an_artists_related_artists) | **GET** /artists/{id}/related-artists | Get Artist's Related Artists 
[**get_an_artists_top_tracks**](ArtistsApi.md#get_an_artists_top_tracks) | **GET** /artists/{id}/top-tracks | Get Artist's Top Tracks 
[**get_followed**](ArtistsApi.md#get_followed) | **GET** /me/following | Get Followed Artists 
[**get_multiple_artists**](ArtistsApi.md#get_multiple_artists) | **GET** /artists | Get Several Artists 
[**unfollow_artists_users**](ArtistsApi.md#unfollow_artists_users) | **DELETE** /me/following | Unfollow Artists or Users 



## check_current_user_follows

> Vec<bool> check_current_user_follows(r#type, ids)
Check If User Follows Artists or Users 

Check to see if the current user is following one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## follow_artists_users

> follow_artists_users(r#type, ids, follow_artists_users_request)
Follow Artists or Users 

Add the current user as a follower of one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |
**follow_artists_users_request** | Option<[**FollowArtistsUsersRequest**](FollowArtistsUsersRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_artist

> models::ArtistObject get_an_artist(id)
Get Artist 

Get Spotify catalog information for a single artist identified by their unique Spotify ID. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |

### Return type

[**models::ArtistObject**](ArtistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_artists_albums

> models::PagingArtistDiscographyAlbumObject get_an_artists_albums(id, include_groups, market, limit, offset)
Get Artist's Albums 

Get Spotify catalog information about an artist's albums. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**include_groups** | Option<**String**> |  |  |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 5]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingArtistDiscographyAlbumObject**](PagingArtistDiscographyAlbumObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_artists_related_artists

> models::GetMultipleArtists200Response get_an_artists_related_artists(id)
Get Artist's Related Artists 

Get Spotify catalog information about artists similar to a given artist. Similarity is based on analysis of the Spotify community's listening history. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |

### Return type

[**models::GetMultipleArtists200Response**](get_multiple_artists_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_artists_top_tracks

> models::GetAnArtistsTopTracks200Response get_an_artists_top_tracks(id, market)
Get Artist's Top Tracks 

Get Spotify catalog information about an artist's top tracks by country. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::GetAnArtistsTopTracks200Response**](get_an_artists_top_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_followed

> models::GetFollowed200Response get_followed(r#type, after, limit)
Get Followed Artists 

Get the current user's followed artists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**after** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]

### Return type

[**models::GetFollowed200Response**](get_followed_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_multiple_artists

> models::GetMultipleArtists200Response get_multiple_artists(ids)
Get Several Artists 

Get Spotify catalog information for several artists based on their Spotify IDs. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

[**models::GetMultipleArtists200Response**](get_multiple_artists_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## unfollow_artists_users

> unfollow_artists_users(r#type, ids, unfollow_artists_users_request)
Unfollow Artists or Users 

Remove the current user as a follower of one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |
**unfollow_artists_users_request** | Option<[**UnfollowArtistsUsersRequest**](UnfollowArtistsUsersRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

